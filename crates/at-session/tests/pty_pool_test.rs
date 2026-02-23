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
