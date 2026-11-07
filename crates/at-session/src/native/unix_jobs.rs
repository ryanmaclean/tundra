//! Bounded observations of ordinary jobs in an anchored Unix session.
//! The caller retains the unreaped session leader and owns final reaping.
//! These observations are not an immutable process sandbox or signal handles.
use std::io;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SessionCensus {
    pub anchor_seen: bool,
    /// Live session members other than the retained leader.
    pub live_members: Vec<libc::pid_t>,
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "freebsd", target_os = "netbsd"))]
mod bsd;

pub(crate) fn census(sid: libc::pid_t) -> io::Result<SessionCensus> {
    if sid <= 1 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid owned session identity"));
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    { linux::census(sid) }
    #[cfg(target_os = "macos")]
    { macos::census(sid) }
    #[cfg(any(target_os = "freebsd", target_os = "netbsd"))]
    { bsd::census(sid) }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "freebsd", target_os = "netbsd")))]
    { Err(io::Error::new(io::ErrorKind::Unsupported, "ordinary session census is unavailable on this target")) }
}

/// Linux's verified readable-proc descriptor signalling route. Other platforms
/// keep their existing observed Pending semantics; no numeric-PID fallback.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn signal_members(sid: libc::pid_t, signal: libc::c_int, cursor: &mut usize) -> io::Result<usize> {
    linux::signal_members(sid, signal, cursor)
}
