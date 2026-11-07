use super::SessionCensus;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;

const MAX_PROCESSES: usize = 65_536;
const MAX_STAT: usize = 16_384;
const MAX_MOUNTINFO: usize = 1_048_576;

fn uncertain(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, message)
}
fn bounded(mut file: File, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    (&mut file).take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit { return Err(uncertain("process observation exceeded its byte bound")); }
    Ok(bytes)
}
fn number(bytes: &[u8]) -> io::Result<u64> {
    if bytes.is_empty() { return Err(uncertain("empty process numeric field")); }
    bytes.iter().try_fold(0_u64, |value, byte| {
        if !byte.is_ascii_digit() { return Err(uncertain("invalid process numeric field")); }
        value.checked_mul(10).and_then(|v| v.checked_add((byte - b'0') as u64))
            .ok_or_else(|| uncertain("process numeric field overflow"))
    })
}
fn pid(bytes: &[u8]) -> io::Result<libc::pid_t> {
    libc::pid_t::try_from(number(bytes)?).map_err(|_| uncertain("process identity overflow"))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Observation {
    pid: libc::pid_t,
    sid: libc::pid_t,
    group: libc::pid_t,
    start: u64,
    zombie: bool,
}
fn parse_stat(bytes: &[u8], expected: libc::pid_t) -> io::Result<Observation> {
    // comm may contain whitespace, newlines and ')' characters. The numeric
    // suffix has no ')', so its boundary is the final closing parenthesis.
    let open = bytes.iter().position(|b| *b == b'(').ok_or_else(|| uncertain("missing process comm"))?;
    let end = bytes.iter().rposition(|b| *b == b')').ok_or_else(|| uncertain("missing process comm end"))?;
    if open < 2 || end < open || bytes[open - 1] != b' ' {
        return Err(uncertain("invalid process stat prefix"));
    }
    if pid(&bytes[..open - 1])? != expected { return Err(uncertain("process stat identity changed")); }
    let fields: Vec<&[u8]> = bytes[end + 1..].split(|b| b.is_ascii_whitespace())
        .filter(|f| !f.is_empty()).collect();
    if fields.len() < 20 || fields[0].len() != 1 {
        return Err(uncertain("short process stat record"));
    }
    let state = fields[0][0];
    if !b"RSDZTtXxKWPI".contains(&state) { return Err(uncertain("unknown process state")); }
    Ok(Observation {
        pid: expected, group: pid(fields[2])?, sid: pid(fields[3])?,
        start: number(fields[19])?, zombie: matches!(state, b'Z' | b'X' | b'x'),
    })
}

fn visible_proc_mount(bytes: &[u8]) -> io::Result<()> {
    let mut found = 0;
    for line in bytes.split(|b| *b == b'\n') {
        let fields: Vec<&[u8]> = line.split(|b| *b == b' ').filter(|f| !f.is_empty()).collect();
        if fields.len() < 6 { continue; }
        if let Some(path) = fields[4].strip_prefix(b"/proc/") {
            let first = path.split(|b| *b == b'/').next().unwrap_or_default();
            if first == b"self" || first == b"thread-self" || (!first.is_empty() && first.iter().all(u8::is_ascii_digit)) {
                return Err(uncertain("process metadata is hidden by an overmount"));
            }
        }
        if fields[4] != b"/proc" { continue; }
        if fields[3] != b"/" { return Err(uncertain("proc census root is a filesystem subtree")); }
        found += 1;
        let separator = fields.iter().position(|f| *f == b"-")
            .ok_or_else(|| uncertain("invalid proc mount record"))?;
        if fields.get(separator + 1) != Some(&b"proc".as_slice()) {
            return Err(uncertain("process census is not backed by procfs"));
        }
        let super_options = fields.get(separator + 3).ok_or_else(|| uncertain("short proc mount record"))?;
        for options in [fields[5], *super_options] {
            for option in options.split(|b| *b == b',') {
                if let Some(value) = option.strip_prefix(b"hidepid=") {
                    if value != b"0" && value != b"off" {
                        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "procfs hides processes from the census"));
                    }
                }
            }
        }
    }
    if found != 1 { return Err(uncertain("proc mount visibility is ambiguous")); }
    Ok(())
}
struct HeldObservation { observation: Observation, directory: File }
struct Snapshot { members: Vec<Observation>, held: Vec<HeldObservation> }
const MAX_SIGNAL_TARGETS: usize = 32;

fn snapshot_bounded(sid: libc::pid_t, selected: &[libc::pid_t]) -> io::Result<Snapshot> {
    visible_proc_mount(&bounded(File::open("/proc/self/mountinfo")?, MAX_MOUNTINFO)?)?;
    let myself = fs::read_link("/proc/self")?;
    let myself = std::os::unix::ffi::OsStrExt::as_bytes(myself.as_os_str());
    if pid(myself)? != unsafe { libc::getpid() } {
        return Err(uncertain("procfs belongs to a different PID namespace"));
    }
    let mut identities = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let name = entry.file_name();
        let bytes = std::os::unix::ffi::OsStrExt::as_bytes(name.as_os_str());
        if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) { continue; }
        let identity = pid(bytes)?;
        if identity <= 0 { return Err(uncertain("invalid proc directory identity")); }
        if identities.len() == MAX_PROCESSES { return Err(uncertain("process census exceeded its record bound")); }
        identities.push(identity);
    }
    identities.sort_unstable();
    if identities.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(uncertain("duplicate process directory"));
    }
    let mut members = Vec::new();
    let mut held = Vec::new();
    for identity in identities {
        // Hold the directory while opening stat: a recycled numeric path cannot
        // redirect this read to another process. Any disappearance invalidates
        // the pass, including a process not yet classified into a session.
        let directory = OpenOptions::new().read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(format!("/proc/{identity}"))?;
        let fd = unsafe { libc::openat(directory.as_raw_fd(), b"stat\0".as_ptr().cast(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW) };
        if fd < 0 { return Err(io::Error::last_os_error()); }
        let record = parse_stat(&bounded(unsafe { File::from_raw_fd(fd) }, MAX_STAT)?, identity)?;
        if record.sid == sid {
            if record.pid != sid && !record.zombie && selected.contains(&record.pid) {
                held.push(HeldObservation { observation: record.clone(), directory });
            }
            members.push(record);
        }
    }
    members.sort_unstable();
    Ok(Snapshot { members, held })
}
fn snapshot(sid: libc::pid_t) -> io::Result<Vec<Observation>> {
    // Ordinary census retains only lightweight records, never one FD per job.
    Ok(snapshot_bounded(sid, &[])?.members)
}

pub(super) fn census(sid: libc::pid_t) -> io::Result<SessionCensus> {
    let first = snapshot(sid)?;
    let second = snapshot(sid)?;
    if first != second { return Err(uncertain("session membership changed during observation")); }
    let anchor_seen = second.iter().any(|p| p.pid == sid && p.group == sid && p.zombie);
    if !anchor_seen { return Err(uncertain("unreaped session anchor is not visible")); }
    Ok(SessionCensus {
        anchor_seen,
        live_members: second.into_iter().filter(|p| p.pid != sid && !p.zombie).map(|p| p.pid).collect(),
    })
}

// This extension has verified syscall bindings only for these existing GNU
// 64-bit targets. Other Linux/Android targets retain explicit Unsupported.
#[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
    any(target_arch = "x86_64", target_arch = "aarch64")))]
fn signal_held(directory: &File, signal: libc::c_int) -> io::Result<()> {
    // The readable proc directory itself is the kernel PID reference. Do not
    // reopen its numeric PID with pidfd_open: that would reintroduce a race.
    let result = unsafe { libc::syscall(libc::SYS_pidfd_send_signal,
        directory.as_raw_fd(), signal, std::ptr::null::<libc::siginfo_t>(), 0_u32) };
    if result == 0 { return Ok(()); }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ENOSYS) {
        return Err(io::Error::new(io::ErrorKind::Unsupported,
            "identity-bound process signalling requires Linux pidfd_send_signal (5.1+, permitted by policy)"));
    }
    Err(error)
}
#[cfg(not(all(target_os = "linux", target_env = "gnu", target_pointer_width = "64",
    any(target_arch = "x86_64", target_arch = "aarch64"))))]
fn signal_held(_directory: &File, _signal: libc::c_int) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported,
        "identity-bound signal bindings not verified for this Linux/Android target"))
}

/// Signal a bounded batch of identity-held ordinary members after the owned
/// leader has exited but remains unreaped. Success is a request, not retirement.
/// The caller must still drain and perform its independent complete census.
pub(super) fn signal_members(sid: libc::pid_t, signal: libc::c_int, cursor: &mut usize) -> io::Result<usize> {
    if sid <= 1 || !matches!(signal, libc::SIGHUP | libc::SIGKILL) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid session retirement signal"));
    }
    let first = snapshot(sid)?;
    let live: Vec<_> = first.iter().filter(|member| member.pid != sid && !member.zombie).collect();
    let selected: Vec<_> = if live.is_empty() { Vec::new() } else {
        let offset = *cursor % live.len();
        let count = MAX_SIGNAL_TARGETS.min(live.len());
        *cursor = (offset + count) % live.len();
        live.iter().cycle().skip(offset).take(count).map(|member| member.pid).collect()
    };
    let second = snapshot_bounded(sid, &selected)?;
    if first != second.members {
        return Err(uncertain("session changed before identity-bound signal batch"));
    }
    if !second.members.iter().any(|observed| observed.pid == sid && observed.group == sid && observed.zombie) {
        return Err(uncertain("unreaped session anchor absent before signalling"));
    }
    let mut signalled = 0;
    let mut first_error = None;
    for member in &second.held {
        // Revalidate membership through this same held directory. A replaced
        // numeric path cannot retarget either this read or the signal syscall.
        let fd = unsafe { libc::openat(member.directory.as_raw_fd(), b"stat\0".as_ptr().cast(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW) };
        if fd < 0 { return Err(io::Error::last_os_error()); }
        let current = parse_stat(&bounded(unsafe { File::from_raw_fd(fd) }, MAX_STAT)?, member.observation.pid)?;
        if current.sid != sid || current.start != member.observation.start || current.group != member.observation.group {
            return Err(uncertain("session member identity or membership changed before signal"));
        }
        if current.zombie { continue; }
        match signal_held(&member.directory, signal) {
            Ok(()) => signalled += 1,
            // The held identity exited; this never authorizes signalling a
            // recycled PID. A later census independently confirms cleanup.
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => {},
            Err(error) => { if first_error.is_none() { first_error = Some(error); } },
        }
    }
    match first_error { Some(error) => Err(error), None => Ok(signalled) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stat(comm: &[u8], state: u8) -> Vec<u8> {
        let mut bytes = b"42 (".to_vec();
        bytes.extend_from_slice(comm);
        bytes.extend_from_slice(b") ");
        bytes.push(state);
        // Fields4..22: ppid,pgrp,sid,...,starttime.
        bytes.extend_from_slice(b" 1 42 42 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 123\n");
        bytes
    }
    #[test]
    fn stat_comm_cannot_shift_session_or_start_fields() {
        let observed = parse_stat(&stat(b"a ) b\n( c)", b'Z'), 42).unwrap();
        assert_eq!((observed.pid, observed.sid, observed.group, observed.start), (42, 42, 42, 123));
        assert!(observed.zombie);
        assert!(parse_stat(&stat(b"same", b'S'), 41).is_err());
        assert!(parse_stat(b"42 (short) S 1 42", 42).is_err());
        assert!(parse_stat(&stat(b"state", b'?'), 42).is_err());
    }
    #[test]
    fn hidden_or_ambiguous_proc_mount_never_means_empty() {
        let normal = b"30 20 0:4 / /proc rw - proc proc rw\n";
        assert!(visible_proc_mount(normal).is_ok());
        assert!(visible_proc_mount(b"30 20 0:4 / /proc rw - proc proc rw,hidepid=2\n").is_err());
        let mut duplicated = normal.to_vec();
        duplicated.extend_from_slice(normal);
        assert!(visible_proc_mount(&duplicated).is_err());
        assert!(visible_proc_mount(b"").is_err());
        assert!(visible_proc_mount(b"30 20 0:4 / /proc rw - proc proc rw\n31 30 0:8 / /proc/42/stat rw - tmpfs tmpfs rw\n").is_err());
        assert!(visible_proc_mount(b"30 20 0:4 /42 /proc rw - proc proc rw\n").is_err());
    }
}

#[cfg(all(test, target_os = "linux", target_env = "gnu", target_pointer_width = "64",
    any(target_arch = "x86_64", target_arch = "aarch64")))]
mod native_signal_tests {
    use super::*;
    #[test]
    #[ignore = "requires admitted remote Linux5.1+ target and native tool graph"]
    fn ordinary_file_is_not_a_process_identity() {
        let file = File::open("/dev/null").unwrap();
        let error = signal_held(&file, libc::SIGHUP).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EBADF));
    }
}
