use at_session::pty_pool::{PtyError, PtyPool};
use std::time::Duration;

#[test]
fn pool_creation_and_capacity() {
    let pool = PtyPool::new(4);
    assert_eq!(pool.max_ptys(), 4);
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn spawn_simple_process() {
    let pool = PtyPool::new(4);
    let handle = pool
        .spawn("/bin/echo", &["hello", "world"], &[])
        .expect("failed to spawn echo");
    assert_eq!(pool.active_count(), 1);

    // Give echo a moment to produce output and exit
    std::thread::sleep(Duration::from_millis(500));

    let output = handle.try_read_all();
    let text = String::from_utf8_lossy(&output);
    assert!(
        text.contains("hello world"),
        "expected 'hello world' in output, got: {text:?}"
    );
}

#[test]
fn read_output_from_spawned_process() {
    let pool = PtyPool::new(4);
    let handle = pool
        .spawn(
            "/bin/sh",
            &["-c", "echo line1; echo line2; echo line3"],
            &[],
        )
        .expect("failed to spawn sh");

    std::thread::sleep(Duration::from_millis(500));

    let output = handle.try_read_all();
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("line1"), "missing line1 in: {text:?}");
    assert!(text.contains("line2"), "missing line2 in: {text:?}");
    assert!(text.contains("line3"), "missing line3 in: {text:?}");
}

#[test]
fn capacity_limit_enforced() {
    let pool = PtyPool::new(2);

    let _h1 = pool.spawn("/bin/cat", &[], &[]).expect("spawn 1");
    let _h2 = pool.spawn("/bin/cat", &[], &[]).expect("spawn 2");
    assert_eq!(pool.active_count(), 2);

    let result = pool.spawn("/bin/cat", &[], &[]);
    assert!(result.is_err(), "expected capacity error");
    match result.unwrap_err() {
        PtyError::AtCapacity { max } => assert_eq!(max, 2),
        other => panic!("expected AtCapacity, got: {other:?}"),
    }
}

#[test]
fn kill_handle_from_pool() {
    let pool = PtyPool::new(4);
    let handle = pool
        .spawn("/bin/cat", &[], &[])
        .expect("failed to spawn cat");
    let hid = handle.id;
    assert_eq!(pool.active_count(), 1);

    handle.kill().expect("failed to kill handle");
    pool.kill(hid).expect("failed to remove from pool");
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn kill_nonexistent_handle_returns_error() {
    let pool = PtyPool::new(4);
    let bogus = uuid::Uuid::new_v4();
    let result = pool.kill(bogus);
    assert!(result.is_err());
    match result.unwrap_err() {
        PtyError::HandleNotFound(id) => assert_eq!(id, bogus),
        other => panic!("expected HandleNotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn read_timeout_returns_data() {
    let pool = PtyPool::new(4);
    let handle = pool
        .spawn("/bin/echo", &["async-test"], &[])
        .expect("failed to spawn echo");

    let data = handle.read_timeout(Duration::from_secs(2)).await;
    assert!(data.is_some(), "expected data from read_timeout");
    let bytes = data.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("async-test"),
        "expected 'async-test' in: {text:?}"
    );
}

#[test]
fn send_and_read_interactive() {
    let pool = PtyPool::new(4);
    let handle = pool
        .spawn("/bin/cat", &[], &[])
        .expect("failed to spawn cat");

    handle.send_line("hello from test").expect("send failed");
    std::thread::sleep(Duration::from_millis(500));

    let output = handle.try_read_all();
    let text = String::from_utf8_lossy(&output);
    assert!(
        text.contains("hello from test"),
        "expected echoed input in: {text:?}"
    );

    handle.kill().expect("kill failed");
}

#[test]
fn resize_pty_succeeds() {
    let pool = PtyPool::new(4);
    let handle = pool
        .spawn("/bin/cat", &[], &[])
        .expect("failed to spawn cat");

    // Resize to various dimensions — should not error.
    handle.resize(120, 40).expect("resize to 120x40 failed");
    handle.resize(80, 24).expect("resize to 80x24 failed");
    handle.resize(200, 60).expect("resize to 200x60 failed");

    handle.kill().expect("kill failed");
}

/// Finding #10: `kill_async` must not stall the async runtime during
/// portable-pty's SIGHUP grace loop, and must reap the child.
#[tokio::test(flavor = "current_thread")]
async fn kill_async_does_not_block_runtime_and_reaps_child() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let pool = PtyPool::new(1);
    // Ignore SIGHUP so portable-pty runs its full ~200ms grace loop before SIGKILL.
    let handle = pool
        .spawn(
            "/bin/sh",
            &["-c", "trap '' HUP; echo ready; while :; do sleep 1; done"],
            &[],
        )
        .expect("spawn sh");
    let mut seen = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !seen.contains("ready") && std::time::Instant::now() < deadline {
        if let Some(chunk) = handle.read_timeout(Duration::from_millis(200)).await {
            seen.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
    assert!(seen.contains("ready"), "shell never became ready: {seen:?}");

    let ticks = Arc::new(AtomicUsize::new(0));
    let t = Arc::clone(&ticks);
    let ticker = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            t.fetch_add(1, Ordering::SeqCst);
        }
    });

    handle.kill_async().await.expect("kill_async failed");
    let observed = ticks.load(Ordering::SeqCst);
    ticker.abort();

    // On a single-threaded runtime a blocking kill would starve the ticker
    // entirely during the ~200ms grace loop.
    assert!(
        observed >= 5,
        "runtime was blocked during kill: only {observed} ticks"
    );
    assert!(!handle.is_alive(), "child still alive after kill_async");
    assert!(handle.exit_code().is_some(), "child not reaped after kill_async");
// ---------------------------------------------------------------------------
// Drop semantics: a dropped PtyHandle kills its child and frees its slot.
// ---------------------------------------------------------------------------

/// `kill -0` succeeds while `pid` exists (including as an unreaped zombie).
fn pid_exists(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Poll until `pid` is gone (killed and reaped) or `timeout` elapses.
fn wait_pid_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if !pid_exists(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !pid_exists(pid)
}

#[test]
fn drop_kills_live_child_and_frees_slot() {
    let pool = PtyPool::new(1);
    let handle = pool.spawn("/bin/cat", &[], &[]).expect("spawn cat");
    let pid = handle.process_id().expect("child pid");
    let reader = handle.reader.clone();
    assert!(handle.is_alive());
    assert!(pid_exists(pid));
    assert_eq!(pool.active_count(), 1);

    drop(handle);

    assert_eq!(pool.active_count(), 0, "drop must free the pool slot");
    assert!(
        wait_pid_gone(pid, Duration::from_secs(5)),
        "child {pid} still exists after handle drop"
    );
    // Child is dead, so the reader thread hits EOF/EIO and closes the channel.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match reader.recv_timeout(Duration::from_millis(100)) {
            Err(flume::RecvTimeoutError::Disconnected) => break,
            _ if std::time::Instant::now() >= deadline => {
                panic!("reader channel still open after drop")
            }
            _ => {}
        }
    }
    // Capacity is usable again.
    let _h = pool
        .spawn("/bin/cat", &[], &[])
        .expect("respawn after drop");
}

#[test]
fn drop_escalates_when_child_ignores_sighup() {
    let pool = PtyPool::new(1);
    // `exec` keeps the pid stable; the trap is inherited as SIG_IGN by sleep.
    let handle = pool
        .spawn("/bin/sh", &["-c", "trap '' HUP; exec sleep 30"], &[])
        .expect("spawn sh");
    let pid = handle.process_id().expect("child pid");
    std::thread::sleep(Duration::from_millis(200));
    assert!(handle.is_alive());

    drop(handle);

    assert_eq!(pool.active_count(), 0);
    assert!(
        wait_pid_gone(pid, Duration::from_secs(5)),
        "SIGHUP-ignoring child {pid} survived handle drop"
    );
}

#[test]
fn drop_after_explicit_cleanup_is_idempotent() {
    // Mirrors the agent executor guard: kill + release, then the handle drops.
    let pool = PtyPool::new(1);
    let h1 = pool.spawn("/bin/cat", &[], &[]).expect("spawn 1");
    let h1_pid = h1.process_id().expect("pid 1");
    let h1_id = h1.id;
    h1.kill().expect("kill 1");
    pool.release(h1.id);
    assert_eq!(pool.active_count(), 0);

    // The freed slot is taken by a new handle before h1 is dropped.
    let h2 = pool.spawn("/bin/cat", &[], &[]).expect("spawn 2");
    assert_eq!(pool.active_count(), 1);

    drop(h1);

    assert_eq!(
        pool.active_count(),
        1,
        "dropping h1 must not free h2's slot"
    );
    assert!(h2.is_alive(), "dropping h1 must not affect h2");
    assert!(wait_pid_gone(h1_pid, Duration::from_secs(5)));
    // h1's id stays freed (Drop did not re-register or double-free it).
    assert!(matches!(
        pool.kill(h1_id),
        Err(PtyError::HandleNotFound(id)) if id == h1_id
    ));
}

#[test]
fn drop_of_exited_child_frees_slot() {
    let pool = PtyPool::new(1);
    let handle = pool.spawn("/bin/echo", &["bye"], &[]).expect("spawn echo");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while handle.is_alive() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(handle.exit_code(), Some(0));
    assert_eq!(pool.active_count(), 1, "exited child still holds its slot");

    drop(handle);
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn drop_after_pool_dropped_still_kills_child() {
    let pool = PtyPool::new(1);
    let handle = pool.spawn("/bin/cat", &[], &[]).expect("spawn cat");
    let pid = handle.process_id().expect("pid");
    drop(pool);

    drop(handle); // must not panic with the pool gone

    assert!(wait_pid_gone(pid, Duration::from_secs(5)));
}
