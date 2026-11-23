//! Terminal lifecycle against real shells in real PTYs.
//!
//! Each test drives the production handlers (`create_terminal`,
//! `delete_terminal`) and the lifecycle owner (`reap_orphans`, `shutdown`)
//! with a capacity-one pool, and checks the OS process table, so a pass
//! means the shell really was killed and reaped and its slot really freed.
#![cfg(unix)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::ApiState;
use at_bridge::terminal_lifecycle::{reap_orphans, shutdown};
use at_bridge::terminal_ws::{create_terminal, delete_terminal};
use at_session::pty_pool::PtyPool;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use uuid::Uuid;

fn state_with_capacity_one() -> (Arc<ApiState>, Arc<PtyPool>) {
    let pool = Arc::new(PtyPool::new(1));
    let state = Arc::new(ApiState::with_pty_pool(EventBus::new(), pool.clone()));
    (state, pool)
}

/// `kill -0`: true while `pid` is in the process table (zombies included).
fn pid_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn wait_pid_gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if !pid_exists(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    !pid_exists(pid)
}

async fn create(state: &Arc<ApiState>) -> StatusCode {
    create_terminal(State(state.clone()))
        .await
        .into_response()
        .status()
}

async fn delete(state: &Arc<ApiState>, id: Uuid) -> StatusCode {
    delete_terminal(State(state.clone()), Path(id.to_string()))
        .await
        .into_response()
        .status()
}

async fn only_terminal(state: &Arc<ApiState>) -> (Uuid, u32) {
    let handles = state.pty_handles.read().await;
    assert_eq!(handles.len(), 1);
    let (id, handle) = handles.iter().next().unwrap();
    (*id, handle.process_id().expect("shell pid"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_kills_real_shell_and_frees_the_only_slot() {
    let (state, pool) = state_with_capacity_one();

    assert_eq!(create(&state).await, StatusCode::CREATED);
    let (id, pid) = only_terminal(&state).await;
    assert!(pid_exists(pid));
    // Registry entry and handle were published together.
    assert!(state.terminal_registry.read().await.get(&id).is_some());

    // The single slot is taken.
    assert_eq!(create(&state).await, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(pool.active_count(), 1);

    assert_eq!(delete(&state, id).await, StatusCode::OK);
    assert!(wait_pid_gone(pid).await, "shell {pid} survived DELETE");
    assert_eq!(pool.active_count(), 0, "DELETE must free the slot");
    assert!(state.pty_handles.read().await.is_empty());
    assert!(state.terminal_registry.read().await.get(&id).is_none());
    assert_eq!(delete(&state, id).await, StatusCode::NOT_FOUND);

    // Next spawn reuses the freed slot.
    assert_eq!(create(&state).await, StatusCode::CREATED);
    let (_, next_pid) = only_terminal(&state).await;
    assert!(shutdown(&state, Duration::from_secs(10)).await.is_empty());
    assert!(wait_pid_gone(next_pid).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reaper_reaps_a_shell_that_exited_by_itself() {
    let (state, pool) = state_with_capacity_one();

    assert_eq!(create(&state).await, StatusCode::CREATED);
    let (id, pid) = only_terminal(&state).await;
    assert_eq!(
        reap_orphans(&state).await,
        0,
        "a live shell is not an orphan"
    );

    state.pty_handles.read().await[&id]
        .send_line("exit 0")
        .expect("send exit");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let exited = matches!(
            state.pty_handles.read().await[&id].status(),
            at_session::pty_pool::ChildStatus::Exited(_)
        );
        if exited {
            break;
        }
        assert!(Instant::now() < deadline, "shell never exited");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    assert_eq!(reap_orphans(&state).await, 1);
    assert!(
        !pid_exists(pid),
        "reaped shell {pid} still in process table"
    );
    assert_eq!(pool.active_count(), 0);
    assert!(state.terminal_registry.read().await.get(&id).is_none());
    assert_eq!(create(&state).await, StatusCode::CREATED);
    assert!(shutdown(&state, Duration::from_secs(10)).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_kills_shells_and_refuses_new_terminals() {
    let (state, pool) = state_with_capacity_one();

    assert_eq!(create(&state).await, StatusCode::CREATED);
    let (id, pid) = only_terminal(&state).await;

    let pending = shutdown(&state, Duration::from_secs(10)).await;
    assert!(pending.is_empty(), "unconfirmed exits: {pending:?}");
    assert!(wait_pid_gone(pid).await, "shell {pid} survived shutdown");
    assert_eq!(pool.active_count(), 0);
    assert!(state.pty_handles.read().await.is_empty());
    assert_eq!(
        state
            .terminal_registry
            .read()
            .await
            .get(&id)
            .map(|t| t.status.clone()),
        Some(at_bridge::terminal::TerminalStatus::Dead)
    );
    assert_eq!(create(&state).await, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(pool.active_count(), 0, "refused create must not spawn");
}

/// Create, DELETE and patrol racing on a capacity-one pool. Whatever the
/// interleaving, every shell that was ever started must end up killed and
/// reaped, the slot must end up free, and no handle may be left behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_create_delete_and_patrol_never_leak_a_child_or_slot() {
    let (state, pool) = state_with_capacity_one();
    let pids = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

    let creator = {
        let state = state.clone();
        let pids = pids.clone();
        tokio::spawn(async move {
            for _ in 0..12 {
                if create(&state).await == StatusCode::CREATED {
                    for handle in state.pty_handles.read().await.values() {
                        if let Some(pid) = handle.process_id() {
                            pids.lock().unwrap().insert(pid);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        })
    };
    let deleter = {
        let state = state.clone();
        tokio::spawn(async move {
            for _ in 0..24 {
                let ids: Vec<Uuid> = state
                    .terminal_registry
                    .read()
                    .await
                    .list()
                    .iter()
                    .map(|t| t.id)
                    .collect();
                for id in ids {
                    let status = delete(&state, id).await;
                    assert!(
                        matches!(
                            status,
                            StatusCode::OK | StatusCode::NOT_FOUND | StatusCode::CONFLICT
                        ),
                        "unexpected DELETE status {status}"
                    );
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
    };
    let patrol = {
        let state = state.clone();
        tokio::spawn(async move {
            for _ in 0..24 {
                reap_orphans(&state).await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
    };
    creator.await.unwrap();
    deleter.await.unwrap();
    patrol.await.unwrap();

    // Ownership invariant: every slot in use belongs to a published handle.
    assert_eq!(
        pool.active_count(),
        state.pty_handles.read().await.len(),
        "pool slots and owned handles diverged"
    );

    assert!(shutdown(&state, Duration::from_secs(10)).await.is_empty());
    assert_eq!(pool.active_count(), 0);
    assert!(state.pty_handles.read().await.is_empty());
    let pids = pids.lock().unwrap().clone();
    assert!(!pids.is_empty(), "no shell was ever started");
    for pid in pids {
        assert!(wait_pid_gone(pid).await, "shell {pid} leaked");
    }
}
