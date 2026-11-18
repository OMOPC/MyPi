//! Windows named pipes: one instance per accepted front end.

use std::ffi::{OsStr, c_void};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Mutex;
use std::time::Duration;

const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;
const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
const PIPE_UNLIMITED_INSTANCES: u32 = 255;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const OPEN_EXISTING: u32 = 3;
const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_BROKEN_PIPE: i32 = 109;
const ERROR_PIPE_BUSY: i32 = 231;
const ERROR_NO_DATA: i32 = 232;
const ERROR_PIPE_CONNECTED: i32 = 535;

const BUFFER: u32 = 64 * 1024;
const RETRIES: usize = 50;

unsafe extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_buffer: u32,
        in_buffer: u32,
        timeout: u32,
        attributes: *mut c_void,
    ) -> *mut c_void;
    fn ConnectNamedPipe(pipe: *mut c_void, overlapped: *mut c_void) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        attributes: *mut c_void,
        creation: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn ReadFile(
        pipe: *mut c_void,
        buffer: *mut u8,
        len: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WriteFile(
        pipe: *mut c_void,
        buffer: *const u8,
        len: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn DuplicateHandle(
        process: *mut c_void,
        source: *mut c_void,
        target_process: *mut c_void,
        target: *mut *mut c_void,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
    fn SetNamedPipeHandleState(
        pipe: *mut c_void,
        mode: *mut u32,
        max_collection: *mut u32,
        timeout: *mut u32,
    ) -> i32;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
    fn CancelIoEx(pipe: *mut c_void, overlapped: *mut c_void) -> i32;
}

/// A pipe name as UTF-16, NUL-terminated.
fn wide(name: &str) -> Vec<u16> {
    OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// One unused pipe instance: the server's door for the next front end.
fn instance(name: &str) -> io::Result<*mut c_void> {
    let wide = wide(name);
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            0, // PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT
            PIPE_UNLIMITED_INSTANCES,
            BUFFER,
            BUFFER,
            0,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(handle)
}

/// The listening end. One instance is kept ready for the next front end.
pub struct Listener {
    name: String,
    path: PathBuf,
    pending: Mutex<usize>,
}

/// One connected end.
pub struct Stream {
    handle: *mut c_void,
}

// A handle is a kernel object, safe to move and to use from either thread its owner picks.
unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

impl Clone for Stream {
    fn clone(&self) -> Stream {
        let mut copy = ptr::null_mut();
        let ok = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.handle,
                GetCurrentProcess(),
                &mut copy,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        assert!(ok != 0, "cannot duplicate the pipe");
        Stream { handle: copy }
    }
}

pub fn bind(name: &str) -> io::Result<Listener> {
    let handle = instance(name)?;
    Ok(Listener {
        name: name.to_string(),
        path: PathBuf::from(name),
        pending: Mutex::new(handle as usize),
    })
}

/// A pipe name is a kernel object, not a file: there is nothing to unlink. The name goes away
/// when the server closes its last handle.
pub fn unlink(_name: &str) {}

impl Listener {
    pub fn accept(&self) -> io::Result<Stream> {
        let pending = {
            let mut slot = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::replace(&mut *slot, 0)
        };
        let handle = if pending == 0 {
            instance(&self.name)?
        } else {
            pending as *mut c_void
        };
        let ok = unsafe { ConnectNamedPipe(handle, ptr::null_mut()) };
        if ok == 0 {
            let error = io::Error::last_os_error();
            // The front end may have connected between creation and the wait.
            if error.raw_os_error() != Some(ERROR_PIPE_CONNECTED) {
                unsafe { CloseHandle(handle) };
                return Err(error);
            }
        }
        // Have the next instance waiting before the loop comes back.
        if let Ok(next) = instance(&self.name) {
            *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = next as usize;
        }
        Ok(Stream { handle })
    }

    pub fn local_path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let handle =
            std::mem::replace(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()), 0);
        if handle != 0 {
            unsafe { CloseHandle(handle as *mut c_void) };
        }
    }
}

pub fn connect(name: &str) -> io::Result<Stream> {
    let wide = wide(name);
    let mut tries = 0;
    loop {
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null_mut(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            let mut mode = 0u32; // PIPE_READMODE_BYTE
            unsafe { SetNamedPipeHandleState(handle, &mut mode, ptr::null_mut(), ptr::null_mut()) };
            return Ok(Stream { handle });
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < RETRIES {
            // Every instance is taken: the server is between accepts. Wait for the next one.
            unsafe { WaitNamedPipeW(wide.as_ptr(), 20) };
            std::thread::sleep(Duration::from_millis(10));
            tries += 1;
            continue;
        }
        return Err(match error.raw_os_error() {
            Some(ERROR_FILE_NOT_FOUND) => io::Error::new(
                io::ErrorKind::NotFound,
                format!("no server listening on {name}"),
            ),
            _ => error,
        });
    }
}

impl Stream {
    /// Close both directions: a blocking read on any clone returns end of file.
    pub fn shutdown(&self) {
        unsafe {
            CancelIoEx(self.handle, ptr::null_mut());
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

impl io::Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut read = 0u32;
        let ok = unsafe {
            ReadFile(
                self.handle,
                out.as_mut_ptr(),
                out.len() as u32,
                &mut read,
                ptr::null_mut(),
            )
        };
        if ok != 0 {
            return Ok(read as usize);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(ERROR_BROKEN_PIPE) | Some(ERROR_NO_DATA) => Ok(0),
            _ => Err(error),
        }
    }
}

impl io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0u32;
        let ok = unsafe {
            WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                ptr::null_mut(),
            )
        };
        if ok != 0 {
            return Ok(written as usize);
        }
        Err(io::Error::last_os_error())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}