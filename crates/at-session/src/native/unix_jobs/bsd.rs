//! Bounded native BSD session census. This module observes; it never signals.
use std::io;
use std::mem::{offset_of, size_of};
use super::SessionCensus;

const MAX_RECORDS: usize = 16_384;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const ATTEMPTS: usize = 4;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn unsupported(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}

fn kernel_major() -> io::Result<u32> {
    let mut name = std::mem::MaybeUninit::<libc::utsname>::zeroed();
    if unsafe { libc::uname(name.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let name = unsafe { name.assume_init() };
    let bytes: Vec<u8> = name.release.iter().map(|c| *c as u8).collect();
    let end = bytes.iter().position(|b| *b == 0)
        .ok_or_else(|| invalid("unterminated kernel release"))?;
    let release = &bytes[..end];
    let split = release.iter().position(|b| !b.is_ascii_digit())
        .ok_or_else(|| invalid("kernel release has no version separator"))?;
    if split == 0 || release[split] != b'.' {
        return Err(invalid("unrecognized kernel release"));
    }
    release[..split].iter().try_fold(0_u32, |major, digit| {
        major.checked_mul(10).and_then(|n| n.checked_add((digit - b'0') as u32))
            .ok_or_else(|| invalid("kernel major version overflow"))
    })
}

fn policy_value(name: &'static [u8], expected: libc::c_int) -> io::Result<()> {
    let mut value: libc::c_int = 0;
    let mut bytes = size_of::<libc::c_int>();
    if unsafe {
        libc::sysctlbyname(name.as_ptr().cast(), (&mut value as *mut libc::c_int).cast(),
            &mut bytes, std::ptr::null_mut(), 0)
    } != 0 {
        return Err(io::Error::last_os_error());
    }
    if bytes != size_of::<libc::c_int>() || value != expected {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied,
            "process visibility policy cannot establish a complete session census"));
    }
    Ok(())
}

#[cfg(target_os = "freebsd")]
fn visibility() -> io::Result<()> {
    policy_value(b"security.bsd.see_other_uids\0", 1)?;
    policy_value(b"security.bsd.see_other_gids\0", 1)?;
    policy_value(b"security.bsd.see_jail_proc\0", 1)
}
#[cfg(target_os = "netbsd")]
fn visibility() -> io::Result<()> {
    policy_value(b"security.models.extensions.curtain\0", 0)
}

// Accept only the verified native record size and consumed field prefix.
// FreeBSD preserves this 64-bit process-record ABI across the selected majors.
#[cfg(target_os = "freebsd")]
fn native_abi() -> io::Result<()> {
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        || !(12..=15).contains(&kernel_major()?)
        || size_of::<libc::kinfo_proc>() != 1088
        || offset_of!(libc::kinfo_proc, ki_structsize) != 0
        || offset_of!(libc::kinfo_proc, ki_layout) != 4
        || offset_of!(libc::kinfo_proc, ki_pid) != 72
        || offset_of!(libc::kinfo_proc, ki_sid) != 88
    {
        return Err(unsupported("unverified FreeBSD process-record ABI"));
    }
    Ok(())
}
#[cfg(target_os = "netbsd")]
fn native_abi() -> io::Result<()> {
    // NetBSD10/11 define the same fixed-width kinfo_proc2 wire structure.
    // Only PID and SID are consumed; all non-anchor rows conservatively count.
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        || !(10..=11).contains(&kernel_major()?)
        || offset_of!(libc::kinfo_proc2, p_pid) != 116
        || offset_of!(libc::kinfo_proc2, p_sid) != 124
    {
        return Err(unsupported("unverified NetBSD process-record ABI"));
    }
    Ok(())
}

#[cfg(target_os = "freebsd")]
type Record = libc::kinfo_proc;
#[cfg(target_os = "netbsd")]
type Record = libc::kinfo_proc2;

#[cfg(target_os = "freebsd")]
fn mib(sid: libc::pid_t, _capacity: usize) -> io::Result<Vec<libc::c_int>> {
    Ok(vec![libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_SESSION, sid])
}
#[cfg(target_os = "netbsd")]
fn mib(sid: libc::pid_t, capacity: usize) -> io::Result<Vec<libc::c_int>> {
    let width = libc::c_int::try_from(size_of::<Record>())
        .map_err(|_| invalid("process record size overflow"))?;
    let count = libc::c_int::try_from(capacity)
        .map_err(|_| invalid("process record count overflow"))?;
    Ok(vec![libc::CTL_KERN, libc::KERN_PROC2, libc::KERN_PROC_SESSION, sid, width, count])
}

fn snapshot(sid: libc::pid_t) -> io::Result<Vec<u8>> {
    let width = size_of::<Record>();
    let limit = MAX_RECORDS.min(MAX_BYTES / width);
    let mut minimum = 0_usize;
    for _ in 0..ATTEMPTS {
        let mut query = mib(sid, limit)?;
        let mut needed = 0_usize;
        if unsafe {
            libc::sysctl(query.as_mut_ptr(), query.len() as _, std::ptr::null_mut(),
                &mut needed, std::ptr::null_mut(), 0)
        } != 0 {
            return Err(io::Error::last_os_error());
        }
        if needed > MAX_BYTES { return Err(invalid("process census exceeds byte limit")); }
        let records = needed.checked_add(width - 1).map(|n| n / width)
            .and_then(|n| n.checked_add(32)).map(|n| n.max(minimum))
            .ok_or_else(|| invalid("process census size overflow"))?;
        if records > limit { return Err(invalid("process census exceeds record limit")); }
        let capacity = records.checked_mul(width)
            .ok_or_else(|| invalid("process census byte-size overflow"))?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity)
            .map_err(|_| io::Error::other("process census allocation failed"))?;
        bytes.resize(capacity, 0_u8);
        let mut used = capacity;
        let mut query = mib(sid, records)?;
        let rc = unsafe {
            libc::sysctl(query.as_mut_ptr(), query.len() as _, bytes.as_mut_ptr().cast(),
                &mut used, std::ptr::null_mut(), 0)
        };
        if rc != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ENOMEM) { return Err(error); }
        } else if used < capacity {
            if used % width != 0 { return Err(invalid("partial native process record")); }
            bytes.truncate(used);
            return Ok(bytes);
        }
        // ENOMEM or an exactly full response is not a complete census.
        minimum = records.checked_mul(2)
            .ok_or_else(|| invalid("process census growth overflow"))?;
    }
    Err(io::Error::new(io::ErrorKind::WouldBlock, "process census kept changing or truncating"))
}

#[cfg(target_os = "freebsd")]
fn identity(record: &Record) -> io::Result<(libc::pid_t, libc::pid_t)> {
    if record.ki_structsize != size_of::<Record>() as libc::c_int || record.ki_layout != 0 {
        return Err(unsupported("kernel process record disagrees with native ABI"));
    }
    Ok((record.ki_pid, record.ki_sid))
}
#[cfg(target_os = "netbsd")]
fn identity(record: &Record) -> io::Result<(libc::pid_t, libc::pid_t)> {
    Ok((record.p_pid, record.p_sid))
}

pub(super) fn census(sid: libc::pid_t) -> io::Result<SessionCensus> {
    if sid <= 1 { return Err(invalid("invalid owned session identifier")); }
    native_abi()?;
    visibility()?;
    let bytes = snapshot(sid)?;
    let mut members = Vec::new();
    members.try_reserve_exact(bytes.len() / size_of::<Record>())
        .map_err(|_| io::Error::other("process census result allocation failed"))?;
    for chunk in bytes.chunks_exact(size_of::<Record>()) {
        // Native C records contain integer/array/raw-pointer fields. The byte
        // buffer need not be aligned; none of its kernel pointers are followed.
        let record = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast::<Record>()) };
        let (pid, record_sid) = identity(&record)?;
        if pid <= 1 || record_sid != sid {
            return Err(invalid("process census returned inconsistent identity"));
        }
        members.push(pid);
    }
    members.sort_unstable();
    if members.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid("duplicate process identity in session census"));
    }
    let anchor = members.binary_search(&sid)
        .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "owned session anchor not visible"))?;
    members.remove(anchor);
    // Conservative: even non-anchor zombies remain members until removed by
    // their real parent. NetBSD p_stat is LWP state, not process state; no state
    // number is interpreted here. Separate process groups are never filtered.
    visibility()?;
    Ok(SessionCensus { anchor_seen: true, live_members: members })
}
