//! Native child-process lifecycle contract for `PtyPool` / `PtyHandle`.
//!
//! Every test here spawns a real OS process inside a real PTY (ConPTY on
//! Windows) and checks the process table directly, so a pass means the child
//! was actually killed and reaped, not that an in-memory fake said so. Runs on
//! Linux, macOS and Windows.

use std::time::{Duration, Instant};

use at_session::pty_pool::{ChildStatus, PtyError, PtyPool};

/// A child that stays alive for ~30s unless killed.
fn long_running() -> (&'static str, Vec<&'static str>) {
    if cfg!(windows) {
        ("ping.exe", vec!["-n", "30", "127.0.0.1"])
    } else {
        ("/bin/sh", vec!["-c", "exec sleep 30"])
    }
}

/// A child that exits by itself with code 3.
fn exits_with_3() -> (&'static str, Vec<&'static str>) {
    if cfg!(windows) {
        ("cmd.exe", vec!["/C", "exit 3"])
    } else {
        ("/bin/sh", vec!["-c", "exit 3"])
    }
}

/// Whether `pid` is still in the OS process table (a zombie counts).
#[cfg(unix)]
fn pid_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Whether `pid` is still in the OS process table.
#[cfg(windows)]
fn pid_exists(pid: u32) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .expect("run tasklist");
    String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    done()
}

/// Capacity one: the running child holds the only slot; terminating it
/// confirms the exit (the pid is gone from the process table) and only then
/// does dropping the handle free the slot for the next spawn.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_one_terminate_reaps_real_child_then_next_spawn_succeeds() {
    let pool = PtyPool::new(1);
    let (cmd, args) = long_running();
    let handle = pool
        .spawn(cmd, &args, &[])
        .expect("spawn long-running child");
    let pid = handle.process_id().expect("child pid");
    assert!(
        pid_exists(pid),
        "child {pid} not in process table after spawn"
    );
    assert_eq!(handle.status(), ChildStatus::Running);

    // The slot is held: a second spawn is refused before any process starts.
    assert!(matches!(
        pool.spawn(cmd, &args, &[]),
        Err(PtyError::AtCapacity { max: 1 })
    ));

    handle.terminate_async().await.expect("terminate");
    assert!(
        matches!(handle.status(), ChildStatus::Exited(_)),
        "exit not observed after terminate"
    );
    assert!(
        wait_until(Duration::from_secs(5), || !pid_exists(pid)),
        "child {pid} still in process table after terminate"
    );
    // Terminating twice is fine: an exited child is not signalled again.
    handle.terminate_async().await.expect("second terminate");

    assert_eq!(
        pool.active_count(),
        1,
        "slot is owned by the handle until drop"
    );
    drop(handle);
    assert_eq!(
        pool.active_count(),
        0,
        "confirmed exit frees the slot on drop"
    );

    let next = pool.spawn(cmd, &args, &[]).expect("next spawn after reap");
    let next_pid = next.process_id().expect("next pid");
    next.terminate_async().await.expect("terminate next");
    assert!(wait_until(Duration::from_secs(5), || !pid_exists(next_pid)));
}

/// A child that exits by itself is reported as `Exited` with its code, and
/// terminating it afterwards returns that code without signalling anything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn natural_exit_is_reported_with_code_and_terminate_does_not_signal() {
    let pool = PtyPool::new(1);
    let (cmd, args) = exits_with_3();
    let handle = pool.spawn(cmd, &args, &[]).expect("spawn exiting child");
    // Keep the PTY drained so the child can never block on output.
    let reader = handle.reader.clone();
    let drain = std::thread::spawn(move || while reader.recv().is_ok() {});

    assert!(
        wait_until(Duration::from_secs(10), || matches!(
            handle.status(),
            ChildStatus::Exited(_)
        )),
        "child never exited"
    );
    assert_eq!(handle.status(), ChildStatus::Exited(3));
    assert_eq!(handle.terminate_async().await.expect("terminate"), 3);

    drop(handle);
    assert_eq!(pool.active_count(), 0);
    let _ = drain.join();
}

/// Dropping a handle whose child is still running kills it; the slot is
/// released once the exit is confirmed, and a new child can be spawned.
#[test]
fn drop_kills_real_child_and_releases_slot_after_confirmed_exit() {
    let pool = PtyPool::new(1);
    let (cmd, args) = long_running();
    let handle = pool
        .spawn(cmd, &args, &[])
        .expect("spawn long-running child");
    let pid = handle.process_id().expect("child pid");
    assert!(pid_exists(pid));

    drop(handle);

    assert!(
        wait_until(Duration::from_secs(10), || pool.active_count() == 0),
        "slot not released after the dropped child exited"
    );
    assert!(
        wait_until(Duration::from_secs(5), || !pid_exists(pid)),
        "child {pid} survived handle drop"
    );
    let next = pool.spawn(cmd, &args, &[]).expect("respawn after drop");
    drop(next);
    assert!(wait_until(Duration::from_secs(10), || pool.active_count() == 0));
}
