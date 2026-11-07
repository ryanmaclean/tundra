//! First-party Unix PTY ownership. All master I/O is nonblocking.
use super::{SpawnFailure, SpawnSpec};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Stdio};

pub(crate) struct NativeSession {
    master: Option<OwnedFd>,
    child: Child,
    exit: Option<ExitStatus>,
    identity_observed: bool,
    reaped: bool,
    last_census: Option<std::time::Instant>,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    last_job_signal: Option<std::time::Instant>,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    job_signal_cursor: usize,
    empty_census_seen: bool,
    eof: bool,
    drained: bool,
    close_error: Option<String>,
    termination_started: Option<std::time::Instant>,
}

fn last_error() -> io::Error { io::Error::last_os_error() }
fn flags(fd: i32, command: i32, value: i32) -> io::Result<()> {
    if unsafe { libc::fcntl(fd, command, value) } < 0 { Err(last_error()) } else { Ok(()) }
}
fn duplicate(fd: i32) -> io::Result<OwnedFd> {
    let new_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if new_fd < 0 { Err(last_error()) } else { Ok(unsafe { OwnedFd::from_raw_fd(new_fd) }) }
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "freebsd", target_os = "netbsd"))]
fn allocate_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    #[cfg(target_os = "netbsd")]
    {
        // NetBSD before 10 silently ignored O_CLOEXEC for posix_openpt.
        // Refuse before allocating; a later fcntl cannot repair inheritance races.
        let mut name = std::mem::MaybeUninit::<libc::utsname>::zeroed();
        if unsafe { libc::uname(name.as_mut_ptr()) } < 0 { return Err(last_error()); }
        let name = unsafe { name.assume_init() };
        let release = unsafe { std::ffi::CStr::from_ptr(name.release.as_ptr()) }.to_bytes();
        let major = release.iter().take_while(|byte| byte.is_ascii_digit()).fold(0_u32, |value, byte| value.saturating_mul(10).saturating_add((byte - b'0') as u32));
        if major < 10 { return Err(io::Error::new(io::ErrorKind::Unsupported, "atomic PTY descriptor acquisition requires NetBSD 10 or newer")); }
    }
    let fd = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
    if fd < 0 { return Err(last_error()); }
    let master = unsafe { OwnedFd::from_raw_fd(fd) };
    if unsafe { libc::grantpt(fd) } < 0 || unsafe { libc::unlockpt(fd) } < 0 { return Err(last_error()); }
    let mut name = vec![0_u8; 4096];
    #[cfg(target_os = "macos")]
    {
        // Darwin TIOCPTYGNAME's primary ABI is a 128-byte output, not ptsname_r.
        if unsafe { libc::ioctl(fd, libc::TIOCPTYGNAME as _, name.as_mut_ptr()) } < 0 { return Err(last_error()); }
        if !name[..128].contains(&0) { return Err(io::Error::other("unterminated PTY name")); }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let result = unsafe { libc::ptsname_r(fd, name.as_mut_ptr().cast(), name.len()) };
        if result != 0 {
            return Err(if result < 0 { last_error() } else { io::Error::from_raw_os_error(result) });
        }
    }
    if !name.contains(&0) { return Err(io::Error::other("unterminated PTY name")); }
    let fd = unsafe { libc::open(name.as_ptr().cast(), libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC) };
    if fd < 0 { return Err(last_error()); }
    Ok((master, unsafe { OwnedFd::from_raw_fd(fd) }))
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "freebsd", target_os = "netbsd")))]
fn allocate_pair() -> io::Result<(OwnedFd, OwnedFd)> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "atomic PTY allocation has not been implemented for this Unix target"))
}

impl NativeSession {
    pub(crate) fn spawn(spec: &SpawnSpec) -> Result<Self, SpawnFailure> {
        Self::prepare(spec).map_err(|error| SpawnFailure { error, retained: None })
    }

    fn prepare(spec: &SpawnSpec) -> io::Result<Self> {
        let (master, slave) = allocate_pair()?;
        let size = libc::winsize { ws_row: spec.rows, ws_col: spec.cols, ws_xpixel: 0, ws_ypixel: 0 };
        if unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ as _, &size) } < 0 { return Err(last_error()); }
        let current = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        if current < 0 { return Err(last_error()); }
        flags(master.as_raw_fd(), libc::F_SETFL, current | libc::O_NONBLOCK)?;
        let mut command = Command::new(&spec.program);
        command.args(&spec.args).envs(spec.env.iter().map(|(k,v)| (k,v)));
        command.stdin(Stdio::from(duplicate(slave.as_raw_fd())?));
        command.stdout(Stdio::from(duplicate(slave.as_raw_fd())?));
        command.stderr(Stdio::from(duplicate(slave.as_raw_fd())?));
        let slave_fd = slave.as_raw_fd();
        // Only async-signal-safe system calls; no allocation or locks here.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 { return Err(io::Error::last_os_error()); }
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // All fallible local setup is complete before spawning. Command owns
        // its exec-error handshake; a returned Child is moved directly to Self.
        let child = command.spawn()?;
        drop(command);
        drop(slave);
        Ok(Self { master: Some(master), child, exit: None, identity_observed: true, reaped: false, last_census: None, #[cfg(any(target_os = "linux", target_os = "android"))] last_job_signal: None, #[cfg(any(target_os = "linux", target_os = "android"))] job_signal_cursor: 0, empty_census_seen: false, eof: false, drained: false, close_error: None, termination_started: None })
    }

    pub(crate) fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.eof { return Ok(0); }
        let fd = self.master.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "PTY closed"))?.as_raw_fd();
        let count = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
        if count > 0 { self.drained = false; return Ok(count as usize); }
        if count == 0 { self.eof = true; self.drained = true; return Ok(0); }
        let error = last_error();
        self.drained = error.kind() == io::ErrorKind::WouldBlock;
        // PTY hangup may be EIO, but only accept it as EOF after child exit
        // has independently been observed. Other errors remain visible.
        if error.raw_os_error() == Some(libc::EIO) && self.exit.is_some() {
            self.eof = true;
            return Ok(0);
        }
        Err(error)
    }

    pub(crate) fn has_pending_write(&self) -> bool { false }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let fd = self.master.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "PTY closed"))?.as_raw_fd();
        let count = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if count < 0 { Err(last_error()) } else { Ok(count as usize) }
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        let fd = self.master.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "PTY closed"))?.as_raw_fd();
        let size = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
        if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &size) } < 0 { Err(last_error()) } else { Ok(()) }
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.reaped { return Ok(self.exit); }
        // WNOWAIT keeps the session leader/PID anchored through group cleanup.
        // std::process::Child::try_wait would reap too early for this purpose.
        let mut observed = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe { libc::waitid(libc::P_PID, self.child.id() as libc::id_t,
            observed.as_mut_ptr(), libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
        if result < 0 { self.identity_observed = false; return Err(last_error()); }
        let observed = unsafe { observed.assume_init() };
        let pid = unsafe { observed.si_pid() };
        if pid == 0 {
            if self.exit.is_some() { self.identity_observed = false; return Err(io::Error::other("exit observation disappeared")); }
            self.identity_observed = true; return Ok(None);
        }
        if pid != self.child.id() as libc::pid_t || observed.si_signo != libc::SIGCHLD {
            self.identity_observed = false;
            return Err(io::Error::other("unexpected waitid identity"));
        }
        let status = unsafe { observed.si_status() };
        let raw = match observed.si_code {
            // waitid may retain full-width exit values (notably NetBSD).
            // Normalize to the low eight bits used by the final wait status.
            libc::CLD_EXITED => (status & 0xff) << 8,
            libc::CLD_KILLED => status & 0x7f,
            libc::CLD_DUMPED => (status & 0x7f) | 0x80,
            _ => { self.identity_observed = false; return Err(io::Error::other("unexpected waitid event")); }
        };
        self.identity_observed = true;
        let status = ExitStatus::from_raw(raw);
        self.exit = Some(status);
        Ok(Some(status))
    }

    pub(crate) fn request_termination(&mut self) -> io::Result<()> {
        let started = *self.termination_started.get_or_insert_with(std::time::Instant::now);
        // Give an ordinary interactive shell its hangup path first so it can
        // notify its jobs. Escalate a still-observed owned process afterwards.
        let signal = if started.elapsed() < std::time::Duration::from_secs(1) { libc::SIGHUP } else { libc::SIGKILL };
        let pid = i32::try_from(self.child.id()).map_err(|_| io::Error::other("child PID overflow"))?;
        let mut groups = Vec::new();
        if !self.reaped && self.identity_observed { groups.push(pid); }
        // Different job-control PGIDs are not protected by this leader's PID
        // anchor. Never signal those numeric groups. Linux may use the separate
        // held-process path below; unsupported targets retain Pending.
        let mut first = None;
        for group in groups {
            if group <= 1 { continue; }
            if unsafe { libc::kill(-group, signal) } < 0 {
                let error = last_error();
                if error.raw_os_error() != Some(libc::ESRCH) && first.is_none() { first = Some(error); }
            }
        }
        if self.exit.is_none() && self.identity_observed && unsafe { libc::kill(pid, signal) } < 0 {
            let error = last_error();
            if error.raw_os_error() != Some(libc::ESRCH) && first.is_none() { first = Some(error); }
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if self.exit.is_some() && self.identity_observed && !self.reaped
            && !self.last_job_signal.is_some_and(|last| last.elapsed() < std::time::Duration::from_millis(100)) {
            self.last_job_signal = Some(std::time::Instant::now());
            // Run before EOF: a surviving job may itself hold the slave open.
            // Partial success never counts as retirement; every failure remains
            // visible while later supervisor ticks retry ownership cleanup.
            if let Err(error) = super::unix_jobs::signal_members(pid, signal, &mut self.job_signal_cursor) {
                if first.is_none() { first = Some(error); }
            }
        }
        match first { Some(error) => Err(error), None => Ok(()) }
    }

    pub(crate) fn poll_close(&mut self) -> io::Result<bool> {
        if let Some(error) = &self.close_error { return Err(io::Error::other(error.clone())); }
        if self.master.is_none() { return Ok(true); }
        if self.exit.is_none() || !self.eof { return Ok(false); }
        if !self.reaped {
            if self.last_census.is_some_and(|last| last.elapsed() < std::time::Duration::from_millis(100)) { return Ok(false); }
            self.last_census = Some(std::time::Instant::now());
            let census = match super::unix_jobs::census(self.child.id() as libc::pid_t) {
                Ok(census) => census,
                Err(error) => { self.empty_census_seen = false; return Err(error); }
            };
            if !census.anchor_seen {
                self.empty_census_seen = false;
                self.identity_observed = false;
                return Err(io::Error::other("owned session anchor absent from census"));
            }
            if !census.live_members.is_empty() { self.empty_census_seen = false; return Ok(false); }
            // Require two complete empty observations on separate supervisor
            // ticks while WNOWAIT still anchors the leader. This is ordinary
            // session cleanup, not a security boundary against hostile forks.
            if !self.empty_census_seen { self.empty_census_seen = true; return Ok(false); }
            match self.child.try_wait() {
                Ok(Some(status)) => { self.exit = Some(status); self.reaped = true; },
                Ok(None) => return Ok(false),
                Err(error) => { self.identity_observed = false; return Err(error); }
            }
        }
        // Direct-child exit plus native EOF are required before closing. A quiet
        // surviving ordinary job is not EOF; WouldBlock retains ownership and
        // capacity while the ordinary session/group retirement path retries.
        let fd = self.master.take().expect("master present").into_raw_fd();
        if unsafe { libc::close(fd) } < 0 {
            let error = last_error();
            // close(EINTR) has target-dependent ownership semantics. Never
            // retry a potentially reused fd; keep capacity Unknown explicitly.
            self.close_error = Some(format!("master close unresolved for descriptor {fd}: {error}"));
            return Err(error);
        }
        self.eof = true;
        Ok(true)
    }
}
