//! Original ConPTY backend. The owner reactor is the sole caller.
//! Pending OVERLAPPED operations own their buffers until observed completion.
//! No pseudoconsole close or thread join is performed by Drop.
use super::{SpawnFailure, SpawnSpec};
use std::ffi::{c_void, OsStr};
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use std::ptr::{null, null_mut};
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL, CSTR_LESS_THAN};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::IO::*;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::*;
use windows_sys::Win32::System::Threading::*;

const CHUNK: usize = 16 * 1024;

fn last() -> io::Error { io::Error::last_os_error() }
fn pending() -> io::Error { io::ErrorKind::WouldBlock.into() }
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut v: Vec<u16> = value.encode_wide().collect();
    if v.contains(&0) { return Err(invalid("NUL in process argument")); }
    v.push(0);
    Ok(v)
}
fn close(handle: &mut HANDLE) -> io::Result<()> {
    if !handle.is_null() {
        if unsafe { CloseHandle(*handle) } == 0 { return Err(last()); }
        *handle = null_mut();
    }
    Ok(())
}
fn coord(rows: u16, cols: u16) -> io::Result<COORD> {
    if rows == 0 || cols == 0 || rows > i16::MAX as u16 || cols > i16::MAX as u16 {
        return Err(invalid("PTY dimensions outside native range"));
    }
    Ok(COORD { X: cols as i16, Y: rows as i16 })
}

// Always quote an argument, doubling backslashes before quotes and at the end.
// This is the Windows C argv convention; no shell syntax is interpolated.
fn quote(value: &str) -> io::Result<Vec<u16>> {
    if value.encode_utf16().count() > 32767 {
        return Err(invalid("Windows argument too long"));
    }
    if value.contains('\0') { return Err(invalid("NUL in process argument")); }
    let mut out = vec![b'"' as u16];
    let mut slashes = 0;
    for ch in value.encode_utf16() {
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        let count = if ch == b'"' as u16 { slashes * 2 + 1 } else { slashes };
        out.extend(std::iter::repeat(b'\\' as u16).take(count));
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat(b'\\' as u16).take(slashes * 2));
    out.push(b'"' as u16);
    Ok(out)
}

fn compare_key(a: &[u16], b: &[u16]) -> io::Result<i32> {
    let alen = i32::try_from(a.len()).map_err(|_| invalid("environment key too long"))?;
    let blen = i32::try_from(b.len()).map_err(|_| invalid("environment key too long"))?;
    let result = unsafe { CompareStringOrdinal(a.as_ptr(), alen, b.as_ptr(), blen, 1) };
    if result == 0 { Err(last()) } else { Ok(result) }
}
fn environment(spec: &SpawnSpec) -> io::Result<Vec<u16>> {
    let mut vars: Vec<(Vec<u16>, Vec<u16>)> = std::env::vars_os()
        .map(|(k, v)| (k.encode_wide().collect(), v.encode_wide().collect()))
        .collect();
    for (key, value) in &spec.env {
        if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
            return Err(invalid("invalid environment override"));
        }
        let key: Vec<u16> = key.encode_utf16().collect();
        let value: Vec<u16> = value.encode_utf16().collect();
        let mut index = 0;
        while index < vars.len() {
            if compare_key(&vars[index].0, &key)? == CSTR_EQUAL {
                vars.remove(index);
            } else {
                index += 1;
            }
        }
        vars.push((key, value));
    }
    // Insertion sort permits native comparison errors to propagate.
    for index in 1..vars.len() {
        let mut current = index;
        while current > 0 && compare_key(&vars[current].0, &vars[current - 1].0)? == CSTR_LESS_THAN {
            vars.swap(current, current - 1);
            current -= 1;
        }
    }
    let mut block = Vec::new();
    for (key, value) in vars {
        if key.contains(&0) || value.contains(&0) { return Err(invalid("NUL in environment")); }
        block.extend(key);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() { block.push(0); }
    block.push(0);
    Ok(block)
}

struct Operation {
    ov: Option<Box<OVERLAPPED>>,
    buffer: Option<Box<[u8; CHUNK]>>,
    event: HANDLE,
    pending: bool,
    len: usize,
    offset: usize,
}
impl Operation {
    fn new() -> io::Result<Self> {
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() { return Err(last()); }
        let mut ov = Box::new(OVERLAPPED::default());
        ov.hEvent = event;
        Ok(Self { ov: Some(ov), buffer: Some(Box::new([0; CHUNK])), event,
            pending: false, len: 0, offset: 0 })
    }
    fn prepare(&mut self) -> io::Result<()> {
        if self.pending { return Err(pending()); }
        if unsafe { ResetEvent(self.event) } == 0 { return Err(last()); }
        let ov = self.ov.as_mut().unwrap();
        **ov = OVERLAPPED::default();
        ov.hEvent = self.event;
        Ok(())
    }
    fn pointer(&mut self) -> *mut OVERLAPPED { &mut **self.ov.as_mut().unwrap() }
    fn finish(&mut self, handle: HANDLE) -> io::Result<usize> {
        let mut count = 0;
        if unsafe { GetOverlappedResult(handle, self.pointer(), &mut count, 0) } != 0 {
            self.pending = false;
            return Ok(count as usize);
        }
        let error = last();
        if error.raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32) {
            return Err(pending());
        }
        // Only a signalled per-operation event proves failed IO has completed.
        // An invalid handle/observation failure must not release its buffer.
        if unsafe { WaitForSingleObject(self.event, 0) } == WAIT_OBJECT_0 {
            self.pending = false;
        }
        Err(error)
    }
    fn cancel(&mut self, handle: HANDLE) -> io::Result<()> {
        if self.pending && unsafe { CancelIoEx(handle, self.pointer()) } == 0 {
            let error = last();
            if error.raw_os_error() != Some(ERROR_NOT_FOUND as i32) { return Err(error); }
        }
        Ok(())
    }
    fn close_event(&mut self) -> io::Result<()> {
        if self.pending { return Err(pending()); }
        close(&mut self.event)
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        if self.pending {
            // Fail-safe for a panicking supervisor: never free memory the kernel
            // may still reference. Normal retirement must observe completion.
            if let Some(ov) = self.ov.take() { std::mem::forget(ov); }
            if let Some(buffer) = self.buffer.take() { std::mem::forget(buffer); }
        } else {
            let _ = close(&mut self.event);
        }
    }
}

/// Exclusively owned by one supervisor. Raw handles and stable boxed operations
/// may move between threads before use, but are never accessed concurrently.
pub(crate) struct NativeSession {
    input: HANDLE,
    output: HANDLE,
    console_input: HANDLE,
    console_output: HANDLE,
    process: HANDLE,
    primary_thread: HANDLE,
    job: HANDLE,
    assigned: bool,
    console: HPCON,
    console_created: bool,
    close_worker: Option<JoinHandle<()>>,
    close_failed: bool,
    reader: Option<Operation>,
    writer: Option<Operation>,
    // Retains a rare pending ConnectNamedPipe operation during failed setup.
    connect: Option<(HANDLE, Operation)>,
    exit: Option<ExitStatus>,
    eof: bool,
    closing: bool,
    termination_requested: bool,
    closed: bool,
}
// SAFETY: ownership is exclusive; kernel IO references only stable heap buffers.
// No API exposes these handles or operation pointers to another caller.
unsafe impl Send for NativeSession {}

impl NativeSession {
    fn empty() -> Self {
        Self { input: null_mut(), output: null_mut(), console_input: null_mut(),
            console_output: null_mut(), process: null_mut(), primary_thread: null_mut(),
            job: null_mut(), assigned: false, console: 0, console_created: false, close_worker: None,
            close_failed: false, reader: None, writer: None, connect: None,
            exit: None, eof: false, closing: false, termination_requested: false, closed: false }
    }

    pub(crate) fn spawn(spec: &SpawnSpec) -> Result<Self, SpawnFailure> {
        let mut session = Self::empty();
        // Keep the partial owner outside the unwind boundary: dropping it
        // inside prepare would lose retryable ownership of acquired handles.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.prepare(spec))) {
            Ok(Ok(())) => Ok(session),
            Ok(Err(error)) => Err(SpawnFailure { error, retained: Some(session) }),
            Err(_) => Err(SpawnFailure {
                error: io::Error::other("native Windows preparation panicked"),
                retained: Some(session),
            }),
        }
    }

    fn make_pipe(&mut self, input: bool) -> io::Result<()> {
        let name = wide(OsStr::new(&format!(r"\\.\pipe\tundra-{}", uuid::Uuid::new_v4())))?;
        // Protected DACL: only SYSTEM and the object's owner have access.
        let descriptor = wide(OsStr::new("D:P(A;;GA;;;SY)(A;;GA;;;OW)"))?;
        let mut security = null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(), SDDL_REVISION_1, &mut security, null_mut(),
        ) } == 0 { return Err(last()); }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security,
            bInheritHandle: 0,
        };
        let access = if input { PIPE_ACCESS_OUTBOUND } else { PIPE_ACCESS_INBOUND };
        let server = unsafe { CreateNamedPipeW(
            name.as_ptr(), access | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            1, CHUNK as u32, CHUNK as u32, 0, &attributes,
        ) };
        let create_error = if server == INVALID_HANDLE_VALUE { Some(last()) } else { None };
        unsafe { LocalFree(security); }
        if let Some(error) = create_error { return Err(error); }
        if input { self.input = server; } else { self.output = server; }

        // Client opens synchronously; only the reactor-facing server is overlapped.
        let client = unsafe { CreateFileW(
            name.as_ptr(), if input { GENERIC_READ } else { GENERIC_WRITE },
            0, null(), OPEN_EXISTING, 0, null_mut(),
        ) };
        if client == INVALID_HANDLE_VALUE { return Err(last()); }
        if input { self.console_input = client; } else { self.console_output = client; }

        self.connect = Some((server, Operation::new()?));
        let (_, connect) = self.connect.as_mut().unwrap();
        connect.prepare()?;
        connect.pending = true;
        let result = unsafe { ConnectNamedPipe(server, connect.pointer()) };
        if result != 0 {
            connect.pending = false;
        } else {
            let error = last();
            match error.raw_os_error().map(|e| e as u32) {
                Some(ERROR_PIPE_CONNECTED) => connect.pending = false,
                Some(ERROR_IO_PENDING) => {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "pipe connection pending"));
                }
                _ => { connect.pending = false; return Err(error); }
            }
        }
        connect.close_event()?;
        self.connect = None;
        Ok(())
    }

    fn prepare(&mut self, spec: &SpawnSpec) -> io::Result<()> {
        let size = coord(spec.rows, spec.cols)?;
        if spec.program.is_empty() || spec.program.contains('"') {
            return Err(invalid("invalid process executable"));
        }
        let mut command = quote(&spec.program)?;
        for arg in &spec.args {
            command.push(b' ' as u16);
            command.extend(quote(arg)?);
        }
        command.push(0);
        if command.len() > 32767 { return Err(invalid("Windows command line too long")); }
        let env = environment(spec)?;
        self.reader = Some(Operation::new()?);
        self.writer = Some(Operation::new()?);
        self.make_pipe(true)?;
        self.make_pipe(false)?;
        let mut console = 0;
        let hr = unsafe { CreatePseudoConsole(
            size, self.console_input, self.console_output, 0, &mut console,
        ) };
        if hr < 0 { return Err(io::Error::other(format!("CreatePseudoConsole HRESULT {hr:#x}"))); }
        // The output handle transfers to us only on successful creation.
        self.console = console;
        self.console_created = true;

        self.job = unsafe { CreateJobObjectW(null(), null()) };
        if self.job.is_null() { return Err(last()); }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe { SetInformationJobObject(
            self.job, JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) } == 0 { return Err(last()); }

        let mut attribute_bytes = 0;
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut attribute_bytes); }
        if attribute_bytes == 0 { return Err(last()); }
        // Match the alignment provided by the Windows heap on 64-bit targets.
        // Vec<usize> alone only guarantees pointer alignment.
        #[repr(C, align(16))]
        #[derive(Clone)]
        struct AttributeBlock { bytes: [u8; 16] }
        let blocks = attribute_bytes.div_ceil(size_of::<AttributeBlock>());
        let mut storage = vec![AttributeBlock { bytes: [0; 16] }; blocks];
        let list = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut attribute_bytes) } == 0 {
            return Err(last());
        }
        // Declared after storage so deletion runs first, including on unwind.
        struct AttributeListGuard(LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for AttributeListGuard {
            fn drop(&mut self) { unsafe { DeleteProcThreadAttributeList(self.0); } }
        }
        let attribute_guard = AttributeListGuard(list);
        // list and its backing storage remain live through CreateProcessW.
        let create_result = (|| {
            if unsafe { UpdateProcThreadAttribute(
                list, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                self.console as *const c_void, size_of::<HPCON>(), null_mut(), null(),
            ) } == 0 { return Err(last()); }
            let mut startup = STARTUPINFOEXW::default();
            startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            startup.lpAttributeList = list;
            let mut process = PROCESS_INFORMATION::default();
            if unsafe { CreateProcessW(
                null(), command.as_mut_ptr(), null(), null(), 0,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                env.as_ptr().cast(), null(), &startup.StartupInfo, &mut process,
            ) } == 0 { return Err(last()); }
            self.process = process.hProcess;
            self.primary_thread = process.hThread;
            Ok(())
        })();
        drop(attribute_guard);
        create_result?;
        if unsafe { AssignProcessToJobObject(self.job, self.process) } == 0 { return Err(last()); }
        self.assigned = true;
        // Remove our duplicate ConPTY-side ends before allowing the child to run.
        close(&mut self.console_input)?;
        close(&mut self.console_output)?;
        if unsafe { ResumeThread(self.primary_thread) } == u32::MAX { return Err(last()); }
        close(&mut self.primary_thread)?;
        Ok(())
    }

    pub(crate) fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if destination.is_empty() { return Ok(0); }
        // Failed acquisition before ConPTY creation has no native producer.
        // Do not issue ReadFile on a server that may never have connected.
        if !self.console_created { self.eof = true; return Ok(0); }
        if self.output.is_null() { self.eof = true; return Ok(0); }
        let reader = self.reader.as_mut().ok_or_else(|| io::Error::other("missing reader"))?;
        if reader.offset == reader.len && !self.eof {
            if !reader.pending {
                reader.prepare()?;
                reader.pending = true;
                let pointer = reader.buffer.as_mut().unwrap().as_mut_ptr();
                let result = unsafe { ReadFile(
                    self.output, pointer, CHUNK as u32, null_mut(), reader.pointer(),
                ) };
                if result == 0 {
                    let error = last();
                    if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                        reader.pending = false;
                        if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                            self.eof = true;
                            return Ok(0);
                        }
                        return Err(error);
                    }
                }
            }
            match reader.finish(self.output) {
                Ok(count) => {
                    reader.len = count;
                    reader.offset = 0;
                    // A zero-byte pipe write can complete a read successfully
                    // with no bytes. Only broken pipe proves producer EOF.
                    if count == 0 { return Err(io::ErrorKind::WouldBlock.into()); }
                }
                Err(error) if !reader.pending && error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => {
                    self.eof = true;
                    return Ok(0);
                }
                Err(error) => return Err(error),
            }
        }
        let count = destination.len().min(reader.len - reader.offset);
        destination[..count].copy_from_slice(
            &reader.buffer.as_ref().unwrap()[reader.offset..reader.offset + count],
        );
        reader.offset += count;
        Ok(count)
    }

    /// Whether the current queue front still owns submitted kernel IO.
    pub(crate) fn has_pending_write(&self) -> bool {
        self.writer.as_ref().is_some_and(|writer| writer.pending)
    }

    pub(crate) fn write(&mut self, source: &[u8]) -> io::Result<usize> {
        if self.closing || self.input.is_null() { return Err(io::ErrorKind::BrokenPipe.into()); }
        if source.is_empty() { return Ok(0); }
        let writer = self.writer.as_mut().ok_or_else(|| io::Error::other("missing writer"))?;
        if writer.pending {
            if source.len() < writer.len || source[..writer.len] != writer.buffer.as_ref().unwrap()[..writer.len] {
                return Err(invalid("pending input must remain at the queue front"));
            }
        } else {
            writer.prepare()?;
            writer.len = source.len().min(CHUNK);
            writer.buffer.as_mut().unwrap()[..writer.len].copy_from_slice(&source[..writer.len]);
            writer.pending = true;
            let pointer = writer.buffer.as_ref().unwrap().as_ptr();
            let result = unsafe { WriteFile(
                self.input, pointer, writer.len as u32, null_mut(), writer.pointer(),
            ) };
            if result == 0 {
                let error = last();
                if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                    writer.pending = false;
                    return Err(error);
                }
            }
        }
        writer.finish(self.input)
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        if self.closing || self.console == 0 { return Err(io::ErrorKind::BrokenPipe.into()); }
        let hr = unsafe { ResizePseudoConsole(self.console, coord(rows, cols)?) };
        if hr < 0 { Err(io::Error::other(format!("ResizePseudoConsole HRESULT {hr:#x}"))) }
        else { Ok(()) }
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(exit) = self.exit { return Ok(Some(exit)); }
        if self.process.is_null() { return Ok(Some(ExitStatus::from_raw(0))); }
        match unsafe { WaitForSingleObject(self.process, 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process, &mut code) } == 0 { return Err(last()); }
                let exit = ExitStatus::from_raw(code);
                self.exit = Some(exit);
                Ok(Some(exit))
            }
            WAIT_FAILED => Err(last()),
            _ => Err(io::Error::other("unexpected process wait result")),
        }
    }

    pub(crate) fn request_termination(&mut self) -> io::Result<()> {
        self.closing = true;
        if let Some(writer) = self.writer.as_mut() { writer.cancel(self.input)?; }
        if self.assigned {
            if unsafe { TerminateJobObject(self.job, 1) } == 0 { return Err(last()); }
        } else if !self.process.is_null() && self.try_wait()?.is_none()
            && unsafe { TerminateProcess(self.process, 1) } == 0 {
            return Err(last());
        }
        self.termination_requested = true;
        Ok(())
    }

    pub(crate) fn poll_close(&mut self) -> io::Result<bool> {
        if self.closed { return Ok(true); }
        if self.close_failed { return Err(io::Error::other("console close worker failed")); }
        if !self.termination_requested { self.request_termination()?; }
        if let Some((handle, operation)) = self.connect.as_mut() {
            operation.cancel(*handle)?;
            if operation.pending {
                match operation.finish(*handle) {
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                    Err(error) if operation.pending => return Err(error),
                    _ => {}
                }
            }
            operation.close_event()?;
            self.connect = None;
        }
        if self.try_wait()?.is_none() { return Ok(false); }
        if self.assigned {
            let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            if unsafe { QueryInformationJobObject(
                self.job, JobObjectBasicAccountingInformation,
                (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32, null_mut(),
            ) } == 0 { return Err(last()); }
            if accounting.ActiveProcesses != 0 { return Ok(false); }
        }
        if let Some(writer) = self.writer.as_mut() {
            writer.cancel(self.input)?;
            if writer.pending {
                match writer.finish(self.input) {
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                    Err(error) if writer.pending => return Err(error),
                    _ => {}
                }
            }
        }
        close(&mut self.input)?;
        close(&mut self.console_input)?;
        close(&mut self.console_output)?;
        if self.console != 0 && self.close_worker.is_none() {
            let console = self.console;
            let worker = thread::Builder::new().name("tundra-conpty-close".into())
                .spawn(move || unsafe { ClosePseudoConsole(console) })?;
            self.close_worker = Some(worker);
        }
        if let Some(worker) = self.close_worker.as_ref() {
            if !worker.is_finished() { return Ok(false); }
            if self.close_worker.take().unwrap().join().is_err() {
                self.close_failed = true;
                return Err(io::Error::other("console close worker failed"));
            }
            self.console = 0;
        }
        // The supervisor must continue read/drain while close runs, and through
        // final EOF. A blocked network client must not own this native reader.
        if !self.console_created { self.eof = true; }
        if !self.output.is_null() && !self.eof { return Ok(false); }
        if let Some(reader) = self.reader.as_mut() {
            if reader.pending || reader.offset != reader.len { return Ok(false); }
            reader.close_event()?;
        }
        if let Some(writer) = self.writer.as_mut() { writer.close_event()?; }
        close(&mut self.output)?;
        close(&mut self.primary_thread)?;
        close(&mut self.process)?;
        close(&mut self.job)?;
        self.closed = true;
        Ok(true)
    }
}

impl Drop for NativeSession {
    fn drop(&mut self) {
        if !self.closed {
            // A logic error/panic must not turn a blocking destructor into a
            // successful retirement, nor free kernel-owned operation buffers.
            // The supervisor is required to retain this object until poll_close.
            tracing::error!("native Windows owner dropped before confirmed retirement");
            if let Some(worker) = self.close_worker.take() { std::mem::forget(worker); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quote_preserves_empty_spaces_quotes_and_trailing_slashes() {
        let decoded = |s| String::from_utf16(&quote(s).unwrap()).unwrap();
        assert_eq!(decoded(""), "\"\"");
        assert_eq!(decoded("a b"), "\"a b\"");
        assert_eq!(decoded("a\"b"), "\"a\\\"b\"");
        assert_eq!(decoded("a\\"), "\"a\\\\\"");
        assert!(quote("a\0b").is_err());
    }
    #[test]
    fn native_dimensions_fail_without_truncation() {
        assert!(coord(0, 80).is_err());
        assert!(coord(24, u16::MAX).is_err());
        let size = coord(24, 80).unwrap();
        assert_eq!(size.X, 80);
        assert_eq!(size.Y, 24);
    }
}
