//! Native lifecycle fixtures: source only until admitted remote target execution.
//! The fixture executable is this Rust test binary; no external shell is used.
use at_session::pty_pool::{Liveness, PtyPool, Retirement};
use std::io::{BufRead, Write};
use std::time::Duration;

#[test]
fn native_child_fixture() {
    let Ok(mode) = std::env::var("TUNDRA_NATIVE_CHILD_MODE") else { return; };
    if mode == "exit" { println!("NATIVE_READY"); return; }
    if mode == "exit-wide" { std::process::exit(256); }
    if mode == "exit-negative" { std::process::exit(-1); }
    #[cfg(unix)]
    if mode == "ignore-hup" || mode == "ignore-hup-group" {
        if mode == "ignore-hup-group" { assert_eq!(unsafe { libc::setpgid(0, 0) }, 0); }
        unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN); }
        println!("CHILD_READY"); std::io::stdout().flush().unwrap();
        loop { std::thread::sleep(Duration::from_millis(50)); }
    }
    #[cfg(unix)]
    if mode == "orphan-redirected" || mode == "orphan-group-redirected" || mode == "orphan-group-slave" {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(fixture_args()).env("TUNDRA_NATIVE_CHILD_MODE",
                if mode == "orphan-redirected" { "ignore-hup" } else { "ignore-hup-group" })
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped())
            .stderr(if mode == "orphan-group-slave" { std::process::Stdio::inherit() } else { std::process::Stdio::null() })
            .spawn().unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        loop { line.clear(); assert!(reader.read_line(&mut line).unwrap() > 0); if line.contains("CHILD_READY") { break; } }
        println!("ORDINARY_CHILD:{}", child.id()); std::io::stdout().flush().unwrap();
        line.clear(); std::io::stdin().read_line(&mut line).unwrap();
        // Natural leader exit. The group-slave mode deliberately retains stderr
        // on the PTY; other modes redirect every standard terminal descriptor.
        return;
    }
    if mode == "flood" {
        for _ in 0..512 { std::io::stdout().write_all(&[b'x'; 4096]).unwrap(); }
        println!("NATIVE_FLOOD_END"); std::io::stdout().flush().unwrap(); return;
    }
    if mode == "ordinary-child" {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(fixture_args()).env("TUNDRA_NATIVE_CHILD_MODE", "hold").spawn().unwrap();
        println!("ORDINARY_CHILD:{}", child.id()); std::io::stdout().flush().unwrap();
        let _ = child.wait(); return;
    }
    if mode == "hold" { loop { std::thread::sleep(Duration::from_millis(50)); } }
    println!("NATIVE_READY"); std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        if line == "quit" { return; }
        println!("REPLY:{line}"); std::io::stdout().flush().unwrap();
    }
}

fn fixture_args() -> [&'static str; 3] { ["--exact", "native_child_fixture", "--nocapture"] }

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_capacity_released_only_after_retirement_then_next_spawn() {
    let executable = std::env::current_exe().unwrap();
    let executable = executable.to_str().unwrap();
    let pool = PtyPool::new(1);
    let handle = pool.spawn_async(executable, &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "echo")]).await.unwrap();
    assert_eq!(pool.active_count(), 1);
    assert!(pool.spawn_async(executable, &fixture_args(), &[]).await.is_err());
    let generation = handle.attach().unwrap();
    handle.resize_connection(generation, 100, 30).await.unwrap();
    handle.connection_input(generation, b"hello\n".to_vec()).await.unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        loop {
            let chunk = handle.connection_output(generation).await.unwrap();
            bytes.extend_from_slice(&chunk.bytes);
            assert!(handle.acknowledge_output(generation, chunk.sequence));
            if String::from_utf8_lossy(&bytes).contains("REPLY:hello") { return bytes; }
        }
    }).await.unwrap();
    assert!(!observed.is_empty());
    handle.kill().unwrap();
    handle.wait_retired(Duration::from_secs(10)).await.unwrap();
    assert_eq!(handle.liveness(), Liveness::Exited);
    assert_eq!(handle.retirement(), Retirement::Retired);
    assert_eq!(pool.active_count(), 0);
    let next = pool.spawn_async(executable, &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "exit")]).await.unwrap();
    next.wait_retired(Duration::from_secs(10)).await.unwrap();
    assert_eq!(pool.active_count(), 0);
}

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_stale_generation_and_oversized_input_are_rejected() {
    let executable = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    let handle = pool.spawn_async(executable.to_str().unwrap(), &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "hold")]).await.unwrap();
    let first = handle.attach().unwrap();
    assert!(handle.detach(first, Duration::from_secs(30), 65536));
    let second = handle.attach().unwrap();
    assert_ne!(first, second);
    assert!(!handle.detach(first, Duration::ZERO, 65536));
    assert!(handle.connection_input(first, vec![1]).await.is_err());
    assert!(tokio::time::timeout(Duration::from_secs(1), handle.connection_input(second, vec![1; 1024*1024+1])).await.unwrap().is_err());
    handle.connection_input(second, Vec::new()).await.unwrap();
    handle.kill().unwrap();
    handle.wait_retired(Duration::from_secs(10)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_last_lease_drop_retires_before_capacity_reuse() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    let first = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "hold")]).await.unwrap();
    let second = first.clone();
    drop(first);
    assert_eq!(pool.active_count(), 1);
    drop(second);
    tokio::time::timeout(Duration::from_secs(10), async {
        while pool.active_count() != 0 { tokio::time::sleep(Duration::from_millis(5)).await; }
    }).await.unwrap();
    let next = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "exit")]).await.unwrap();
    next.wait_retired(Duration::from_secs(10)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_concurrent_reservation_failure_and_shutdown() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    let env = [("TUNDRA_NATIVE_CHILD_MODE", "hold")];
    let args = fixture_args();
    let (left, right) = tokio::join!(pool.spawn_async(exe.to_str().unwrap(), &args, &env), pool.spawn_async(exe.to_str().unwrap(), &args, &env));
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert_eq!(pool.active_count(), 1);
    assert!(pool.shutdown(Duration::from_secs(10)).await.is_empty());
    assert_eq!(pool.active_count(), 0);
    assert!(pool.spawn_async(exe.to_str().unwrap(), &args, &env).await.is_err());
    let pool = PtyPool::new(1);
    let missing = exe.with_file_name("tundra-nonexistent-native-fixture-program");
    assert!(pool.spawn_async(missing.to_str().unwrap(), &[], &[]).await.is_err());
    tokio::time::timeout(Duration::from_secs(10), async {
        while pool.active_count() != 0 { tokio::time::sleep(Duration::from_millis(5)).await; }
    }).await.unwrap();
    let next = pool.spawn_async(exe.to_str().unwrap(), &args, &[("TUNDRA_NATIVE_CHILD_MODE", "exit")]).await.unwrap();
    next.wait_retired(Duration::from_secs(10)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_unattached_output_is_bounded_and_natural_exit_drains_tail() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    let child = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "flood")]).await.unwrap();
    child.set_disconnect_buffer(65536);
    child.wait_retired(Duration::from_secs(10)).await.unwrap();
    let tail = child.try_read_all();
    assert!(tail.len() <= 65536);
    assert!(String::from_utf8_lossy(&tail).contains("NATIVE_FLOOD_END"));
    assert!(child.lost_output_bytes() > 0);
    assert_eq!(pool.active_count(), 0);
}

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_stale_resize_and_grace_do_not_retire_successor() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    let child = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "echo")]).await.unwrap();
    let old = child.attach().unwrap();
    child.detach(old, Duration::from_millis(100), 65536);
    let current = child.attach().unwrap();
    assert!(child.resize_connection(old, 10, 10).await.is_err());
    child.resize_connection(current, 100, 30).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(child.liveness(), Liveness::Alive);
    child.kill().unwrap();
    child.wait_retired(Duration::from_secs(10)).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires admitted remote Unix native target/tool graph"]
async fn native_natural_exit_retires_redirected_hup_ignoring_ordinary_job() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    let leader = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(), &[("TUNDRA_NATIVE_CHILD_MODE", "orphan-redirected")]).await.unwrap();
    let job_pid: libc::pid_t = tokio::time::timeout(Duration::from_secs(10), async {
        let mut output = String::new();
        loop {
            if let Some(chunk) = leader.read_timeout(Duration::from_secs(1)).await {
                output.push_str(&String::from_utf8_lossy(&chunk));
                if let Some(rest) = output.split("ORDINARY_CHILD:").nth(1) {
                    if let Some(end) = rest.find(['\r','\n']) { return rest[..end].parse().unwrap(); }
                }
            }
        }
    }).await.unwrap();
    assert!(job_pid > 1);
    assert_eq!(unsafe { libc::kill(job_pid, 0) }, 0);
    assert_eq!(pool.active_count(), 1);
    leader.send_async(b"quit\n").await.unwrap();
    leader.wait_retired(Duration::from_secs(15)).await.unwrap();
    // Independent native observation: do not accept a live surviving PID merely
    // because the owner reported completion. Zombie reaping may lag briefly.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if unsafe { libc::kill(job_pid, 0) } < 0 {
                assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert_eq!(pool.active_count(), 0);
}

#[tokio::test]
#[ignore = "requires admitted remote native target/tool graph"]
async fn native_cancelled_spawn_owner_retires_reserved_capacity() {
    let exe = std::env::current_exe().unwrap();
    let pool = std::sync::Arc::new(PtyPool::new(1));
    let launch_pool = pool.clone();
    let launch_exe = exe.clone();
    let launch = tokio::spawn(async move {
        launch_pool.spawn_async(launch_exe.to_str().unwrap(), &fixture_args(),
            &[("TUNDRA_NATIVE_CHILD_MODE", "hold")]).await.unwrap()
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while pool.active_count() == 0 { tokio::task::yield_now().await; }
    }).await.unwrap();
    // Abort either a still-preparing future or its completed, unclaimed handle.
    // Both own the reservation; timing does not assert which phase was reached.
    launch.abort();
    if let Ok(handle) = launch.await { drop(handle); }
    tokio::time::timeout(Duration::from_secs(10), async {
        while pool.active_count() != 0 { tokio::time::sleep(Duration::from_millis(5)).await; }
    }).await.unwrap();
    let next = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(),
        &[("TUNDRA_NATIVE_CHILD_MODE", "exit")]).await.unwrap();
    next.wait_retired(Duration::from_secs(10)).await.unwrap();
    assert_eq!(pool.active_count(), 0);
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires admitted remote Unix native target/tool graph"]
async fn native_waitid_exit_codes_match_final_wait_semantics() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    for (mode, expected) in [("exit", 0), ("exit-wide", 0), ("exit-negative", 255)] {
        let child = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(),
            &[("TUNDRA_NATIVE_CHILD_MODE", mode)]).await.unwrap();
        child.wait_retired(Duration::from_secs(10)).await.unwrap();
        assert_eq!(child.exit_code(), Some(expected));
        assert_eq!(pool.active_count(), 0);
    }
    let child = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(),
        &[("TUNDRA_NATIVE_CHILD_MODE", "hold")]).await.unwrap();
    child.kill().unwrap();
    child.wait_retired(Duration::from_secs(10)).await.unwrap();
    assert_eq!(child.exit_code(), None, "signal termination must not invent exit code zero");
}

#[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
    any(target_arch = "x86_64", target_arch = "aarch64")))]
#[tokio::test]
#[ignore = "requires admitted remote Linux5.1+ target, complete proc visibility and full tool graph"]
async fn native_separate_job_group_is_retired_with_and_without_slave_fds() {
    let exe = std::env::current_exe().unwrap();
    let pool = PtyPool::new(1);
    for mode in ["orphan-group-redirected", "orphan-group-slave"] {
        let leader = pool.spawn_async(exe.to_str().unwrap(), &fixture_args(),
            &[("TUNDRA_NATIVE_CHILD_MODE", mode)]).await.unwrap();
        let job: libc::pid_t = tokio::time::timeout(Duration::from_secs(10), async {
            let mut output = String::new();
            loop {
                if let Some(chunk) = leader.read_timeout(Duration::from_secs(1)).await {
                    output.push_str(&String::from_utf8_lossy(&chunk));
                    if let Some(rest) = output.split("ORDINARY_CHILD:").nth(1) {
                        if let Some(end) = rest.find(['\r','\n']) { return rest[..end].parse().unwrap(); }
                    }
                }
            }
        }).await.unwrap();
        assert!(job > 1);
        assert_eq!(unsafe { libc::getpgid(job) }, job, "fixture must have a distinct job-control group");
        assert_eq!(unsafe { libc::kill(job, 0) }, 0);
        assert_eq!(pool.active_count(), 1);
        leader.send_async(b"quit\n").await.unwrap();
        leader.wait_retired(Duration::from_secs(15)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if unsafe { libc::kill(job, 0) } < 0 {
                    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        assert_eq!(pool.active_count(), 0);
    }
}
