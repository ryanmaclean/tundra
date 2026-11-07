//! Original native backends. No child or blocking I/O ownership is exported.
use std::io;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
pub(crate) use unix::NativeSession;
#[cfg(windows)]
pub(crate) use windows::NativeSession;

#[derive(Clone, Debug)]
pub(crate) struct SpawnSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub rows: u16,
    pub cols: u16,
}

/// A failed launch may still own a process, pending operation or console.
/// The supervisor must retire that object before releasing its reservation.
pub(crate) struct SpawnFailure {
    pub error: io::Error,
    pub retained: Option<NativeSession>,
}

#[cfg(unix)]
mod unix_jobs;
