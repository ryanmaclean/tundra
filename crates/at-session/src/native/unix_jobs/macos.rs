use super::SessionCensus;
use std::io;
use std::mem::{size_of, MaybeUninit};

const MAX_PROCESSES: usize = 65_536;
// Official XNU proc_info.h: public proc_listpids selector. This constant is
// absent from the selected libc module, unlike PROC_PIDTBSDINFO.
const PROC_ALL_PIDS: u32 = 1;

fn uncertain(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, message)
}
fn list_pids() -> io::Result<Vec<libc::pid_t>> {
    let needed = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 { return Err(uncertain("macOS process-list size unavailable")); }
    let width = size_of::<libc::pid_t>();
    if needed as usize % width != 0 { return Err(uncertain("unaligned macOS process-list size")); }
    let mut count = (needed as usize / width).checked_add(64)
        .ok_or_else(|| uncertain("macOS process-list size overflow"))?;
    for _ in 0..4 {
        if count > MAX_PROCESSES { return Err(uncertain("macOS process census exceeded its record bound")); }
        let mut pids = vec![0; count];
        let bytes = count.checked_mul(width).and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| uncertain("macOS process-list byte size overflow"))?;
        let used = unsafe { libc::proc_listpids(PROC_ALL_PIDS, 0, pids.as_mut_ptr().cast(), bytes) };
        // libproc returns zero on syscall failure; it is not an empty census.
        if used <= 0 || used as usize % width != 0 {
            return Err(uncertain("macOS process list failed or returned malformed bytes"));
        }
        if used >= bytes {
            count = count.checked_mul(2).ok_or_else(|| uncertain("macOS process-list growth overflow"))?;
            continue;
        }
        pids.truncate(used as usize / width);
        if pids.iter().any(|pid| *pid < 0) { return Err(uncertain("invalid macOS process identity")); }
        pids.retain(|pid| *pid > 0); // PID0 is the kernel, never this anchored session.
        pids.sort_unstable();
        if pids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(uncertain("duplicate macOS process identity"));
        }
        return Ok(pids);
    }
    Err(uncertain("macOS process list kept growing"))
}
fn info(pid: libc::pid_t) -> io::Result<libc::proc_bsdinfo> {
    let mut record = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = i32::try_from(size_of::<libc::proc_bsdinfo>())
        .map_err(|_| uncertain("macOS process record size overflow"))?;
    // XNU BSDINFO only searches zombproc when this argument is nonzero.
    // WNOWAIT deliberately leaves our leader there until cleanup completes.
    let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 1, record.as_mut_ptr().cast(), size) };
    if read != size { return Err(uncertain("macOS process record unavailable or incomplete")); }
    let record = unsafe { record.assume_init() };
    if record.pbi_pid != pid as u32 || !(1..=5).contains(&record.pbi_status) {
        return Err(uncertain("macOS process record identity or state mismatch"));
    }
    Ok(record)
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Observation {
    pid: libc::pid_t,
    start_seconds: u64,
    start_micros: u64,
    zombie: bool,
}
fn observation(info: &libc::proc_bsdinfo) -> Observation {
    Observation {
        pid: info.pbi_pid as libc::pid_t,
        start_seconds: info.pbi_start_tvsec,
        start_micros: info.pbi_start_tvusec,
        zombie: info.pbi_status == libc::SZOMB,
    }
}
fn snapshot(sid: libc::pid_t) -> io::Result<Vec<Observation>> {
    let mut members = Vec::new();
    for pid in list_pids()? {
        let before = info(pid)?;
        if pid == sid {
            // getsid uses a live-process lookup on Darwin. The caller instead
            // retains this exact exited child with WNOWAIT, while BSDINFO
            // explicitly supports zombies. No numeric signal is sent here.
            if before.pbi_status != libc::SZOMB {
                return Err(uncertain("macOS session anchor is not an observed zombie"));
            }
            members.push(observation(&before));
            continue;
        }
        // A positively observed zombie cannot execute or fork. It need not
        // remain findable by getsid, and is not a live ordinary job.
        if before.pbi_status == libc::SZOMB { continue; }
        let session = unsafe { libc::getsid(pid) };
        if session < 0 { return Err(io::Error::last_os_error()); }
        let after = info(pid)?;
        if before.pbi_pid != after.pbi_pid
            || before.pbi_start_tvsec != after.pbi_start_tvsec
            || before.pbi_start_tvusec != after.pbi_start_tvusec {
            return Err(uncertain("macOS process identity changed during session lookup"));
        }
        if session == sid { members.push(observation(&after)); }
    }
    members.sort_unstable();
    Ok(members)
}

pub(super) fn census(sid: libc::pid_t) -> io::Result<SessionCensus> {
    let first = snapshot(sid)?;
    let second = snapshot(sid)?;
    if first != second { return Err(uncertain("macOS session membership changed during observation")); }
    // The inspected XNU enumeration visits allproc before zombproc. Seeing
    // our zombie anchor therefore also guards against an internally capped
    // list that stopped before traversing live rows. Target behavior still
    // requires native verification; this is not an atomic fork-proof census.
    let anchor_seen = second.iter().any(|p| p.pid == sid && p.zombie);
    if !anchor_seen { return Err(uncertain("macOS unreaped session anchor is not visible")); }
    Ok(SessionCensus {
        anchor_seen,
        live_members: second.into_iter().filter(|p| p.pid != sid && !p.zombie).map(|p| p.pid).collect(),
    })
}
