//! Proves the executor heartbeat reaches the daemon's live agent registry and
//! that the stuck-agent patrol (`[daemon.patrol]`, on by default) spares an
//! executor whose CLI is silent but alive while still killing an agent that
//! stops heartbeating.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use at_agents::executor::{AgentExecutor, ProcessControl, PtySpawner, SpawnedProcess};
use at_agents::profiles::AgentConfig;
use at_bridge::protocol::{BridgeMessage, EVENT_AGENT_FORCE_KILL, EXECUTOR_AGENT_SCHEMA};
use at_core::cache::CacheDb;
use at_core::config::Config;
use at_core::types::{
    Agent, AgentRole, AgentStatus, CliType, Task, TaskCategory, TaskComplexity, TaskPhase,
    TaskPriority,
};
use at_daemon::daemon::{Daemon, DaemonIntervals};
use chrono::Utc;
use uuid::Uuid;

/// How long the fake CLI stays silent before printing and exiting 0.
const SILENT_FOR: Duration = Duration::from_secs(6);
const PING_TIMEOUT_SECS: u64 = 2;

#[derive(Default)]
struct ProcState {
    alive: AtomicBool,
    exit_code: Mutex<Option<i32>>,
    killed: AtomicBool,
}

struct Control(Arc<ProcState>);

impl ProcessControl for Control {
    fn is_alive(&self) -> bool {
        self.0.alive.load(Ordering::SeqCst)
    }
    fn exit_code(&self) -> Option<i32> {
        *self.0.exit_code.lock().unwrap()
    }
    fn kill(&self) {
        self.0.killed.store(true, Ordering::SeqCst);
        self.0.alive.store(false, Ordering::SeqCst);
        *self.0.exit_code.lock().unwrap() = Some(137);
    }
    fn release(&self) {}
}

/// A `claude --print`-like process: no output for `SILENT_FOR`, then one
/// line and exit 0 (unless killed first).
struct SilentThenExit {
    state: Arc<ProcState>,
    stdin: Mutex<Vec<flume::Receiver<Vec<u8>>>>,
}

impl PtySpawner for SilentThenExit {
    fn spawn(
        &self,
        _cmd: &str,
        _args: &[&str],
        _env: &[(&str, &str)],
    ) -> Result<SpawnedProcess, String> {
        let (out_tx, out_rx) = flume::bounded(16);
        let (in_tx, in_rx) = flume::bounded(16);
        self.stdin.lock().unwrap().push(in_rx);
        self.state.alive.store(true, Ordering::SeqCst);
        let state = Arc::clone(&self.state);
        std::thread::spawn(move || {
            std::thread::sleep(SILENT_FOR);
            if state.killed.load(Ordering::SeqCst) {
                return;
            }
            let _ = out_tx.send(b"done\n".to_vec());
            *state.exit_code.lock().unwrap() = Some(0);
            state.alive.store(false, Ordering::SeqCst);
        });
        Ok(SpawnedProcess::with_control(
            Uuid::new_v4(),
            out_rx,
            in_tx,
            Box::new(Control(Arc::clone(&self.state))),
        ))
    }
}

fn task() -> Task {
    Task::new(
        "silent but alive",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    )
}

async fn registry_agent(daemon: &Daemon, id: Uuid) -> Option<Agent> {
    daemon.api_state().agents.read().await.get(&id).cloned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn patrol_spares_heartbeating_executor_and_kills_silent_agent() {
    let mut config = Config::default();
    assert!(
        config.daemon.patrol.enabled,
        "patrol is on by default now that executors heartbeat"
    );
    config.daemon.patrol.ping_timeout_secs = PING_TIMEOUT_SECS;
    config.daemon.patrol.consecutive_failures = 1;
    config.terminal.pty_pool_enabled = false;

    let cache = Arc::new(CacheDb::new_in_memory().await.expect("cache"));
    let mut daemon = Daemon::with_cache(config, cache);
    daemon.set_intervals(DaemonIntervals {
        patrol_secs: 3600,
        heartbeat_secs: 1,
        kpi_secs: 3600,
    });
    daemon.spawn_background_loops();
    let bus = daemon.event_bus().clone();
    let rx = bus.subscribe();

    // Control: a live agent (has a session) that never heartbeats.
    let mut silent = Agent::new("never-beats", AgentRole::Crew, CliType::Claude);
    silent.status = AgentStatus::Active;
    silent.session_id = Some("stale-session".into());
    bus.publish(BridgeMessage::AgentCreated(silent.clone()));

    let state = Arc::new(ProcState::default());
    let spawner = Arc::new(SilentThenExit {
        state: Arc::clone(&state),
        stdin: Mutex::new(Vec::new()),
    });
    let executor = Arc::new(
        AgentExecutor::with_spawner(spawner, bus.clone())
            .with_heartbeat_interval(Duration::from_millis(100)),
    );
    let mut agent_config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    agent_config.timeout_secs = 60;
    let run = {
        let executor = Arc::clone(&executor);
        let task = task();
        tokio::spawn(async move { executor.execute_task(&task, &agent_config).await })
    };

    // The executor's agent shows up in the daemon's registry via the bus.
    let exec_id = loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv_async())
            .await
            .expect("executor registers its agent")
            .unwrap();
        if let BridgeMessage::AgentCreated(a) = &*msg {
            if a.id != silent.id {
                break a.id;
            }
        }
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let early = registry_agent(&daemon, exec_id)
        .await
        .expect("executor agent in ApiState.agents");
    assert_eq!(early.status, AgentStatus::Active);
    assert_eq!(
        early.metadata.as_ref().unwrap()["schema"],
        EXECUTOR_AGENT_SCHEMA
    );

    // Past several patrol ticks and well past the ping timeout.
    tokio::time::sleep(Duration::from_secs(PING_TIMEOUT_SECS + 2)).await;
    let mid = registry_agent(&daemon, exec_id).await.unwrap();
    assert_eq!(
        mid.status,
        AgentStatus::Active,
        "a heartbeating executor must not be force-killed"
    );
    assert!(
        mid.last_seen > early.last_seen,
        "idle reads must advance last_seen"
    );
    assert!(
        Utc::now().signed_duration_since(mid.last_seen) < chrono::Duration::seconds(1),
        "last_seen should be fresh"
    );

    // The non-heartbeating control agent was killed: the patrol is live.
    let control = registry_agent(&daemon, silent.id).await.unwrap();
    assert_eq!(control.status, AgentStatus::Stopped);

    let result = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("execution finishes")
        .unwrap()
        .unwrap();
    assert!(
        result.success,
        "silent-but-alive CLI must run to completion"
    );
    assert!(!state.killed.load(Ordering::SeqCst));

    // Exit event lands in the registry.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let done = loop {
        let a = registry_agent(&daemon, exec_id).await.unwrap();
        if a.status == AgentStatus::Stopped || tokio::time::Instant::now() > deadline {
            break a;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(done.status, AgentStatus::Stopped);
    assert_eq!(done.metadata.as_ref().unwrap()["exit"]["success"], true);

    let kills: Vec<Option<Uuid>> = rx
        .try_iter()
        .filter_map(|m| match &*m {
            BridgeMessage::Event(p) if p.event_type == EVENT_AGENT_FORCE_KILL => Some(p.agent_id),
            _ => None,
        })
        .collect();
    assert!(
        !kills.contains(&Some(exec_id)),
        "executor agent never killed"
    );

    daemon.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn patrol_force_kill_aborts_executor_that_stops_heartbeating() {
    // An executor whose heartbeats are spaced beyond the ping timeout is
    // indistinguishable from a wedged one: the patrol kills it and the
    // executor aborts the process on the force-kill event.
    let mut config = Config::default();
    config.daemon.patrol.ping_timeout_secs = 1;
    config.daemon.patrol.consecutive_failures = 1;
    config.terminal.pty_pool_enabled = false;

    let cache = Arc::new(CacheDb::new_in_memory().await.expect("cache"));
    let mut daemon = Daemon::with_cache(config, cache);
    daemon.set_intervals(DaemonIntervals {
        patrol_secs: 3600,
        heartbeat_secs: 1,
        kpi_secs: 3600,
    });
    daemon.spawn_background_loops();
    let bus = daemon.event_bus().clone();

    let state = Arc::new(ProcState::default());
    let spawner = Arc::new(SilentThenExit {
        state: Arc::clone(&state),
        stdin: Mutex::new(Vec::new()),
    });
    let executor = AgentExecutor::with_spawner(spawner, bus.clone())
        .with_heartbeat_interval(Duration::from_secs(3600));
    let mut agent_config = AgentConfig::default_for_phase(CliType::Claude, TaskPhase::Coding);
    agent_config.timeout_secs = 60;

    let result = tokio::time::timeout(
        SILENT_FOR - Duration::from_millis(500),
        executor.execute_task(&task(), &agent_config),
    )
    .await
    .expect("patrol force-kill must end the run before the CLI would exit")
    .unwrap();
    assert!(!result.success);
    assert!(state.killed.load(Ordering::SeqCst), "process was killed");

    daemon.shutdown();
}
