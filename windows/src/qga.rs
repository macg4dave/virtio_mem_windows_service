use std::time::Duration;

use crate::runtime::GuestAgent;

const GET_MEMORY_STATS_REQUEST: &str = r#"{"execute":"guest-get-memory-stats"}"#;
pub const DEFAULT_QGA_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct NamedPipeGuestAgent {
    pipe_path: String,
    operation_timeout: Duration,
}

impl NamedPipeGuestAgent {
    pub fn new(pipe_path: impl Into<String>) -> Self {
        Self::with_operation_timeout(pipe_path, DEFAULT_QGA_OPERATION_TIMEOUT)
    }

    pub fn with_operation_timeout(pipe_path: impl Into<String>, timeout: Duration) -> Self {
        Self {
            pipe_path: pipe_path.into(),
            operation_timeout: timeout,
        }
    }
}

impl GuestAgent for NamedPipeGuestAgent {
    fn get_memory_stats(&mut self) -> Result<String, String> {
        if self.operation_timeout.is_zero() {
            return Err("QEMU Guest Agent operation timeout must be greater than zero".to_owned());
        }

        request_memory_stats(&self.pipe_path, self.operation_timeout)
    }
}

#[cfg(windows)]
fn request_memory_stats(pipe_path: &str, timeout: Duration) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use std::time::Instant;

    use winapi::shared::minwindef::{FALSE, TRUE};
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::namedpipeapi::WaitNamedPipeW;
    use winapi::um::synchapi::CreateEventW;
    use winapi::um::winbase::FILE_FLAG_OVERLAPPED;
    use winapi::um::winnt::{FILE_ATTRIBUTE_NORMAL, GENERIC_READ, GENERIC_WRITE};

    let started = Instant::now();
    let wide_path: Vec<u16> = std::ffi::OsStr::new(pipe_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let timeout_ms = duration_to_timeout_ms(timeout);

    // QEMU's Windows virtio-serial channel is exposed as a device path under
    // `\\.\Global\...`, not as a Win32 named pipe under `\\.\pipe\...`.
    // WaitNamedPipeW rejects the device path with ERROR_BAD_PATHNAME (161),
    // so only use it for actual named-pipe endpoints.
    if pipe_path.to_ascii_lowercase().starts_with(r"\\.\pipe\")
        && unsafe { WaitNamedPipeW(wide_path.as_ptr(), timeout_ms) } == FALSE
    {
        return Err(format!("wait for {pipe_path}: {}", unsafe {
            GetLastError()
        }));
    }

    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("open {pipe_path}: {}", unsafe { GetLastError() }));
    }

    let event = unsafe { CreateEventW(null_mut(), TRUE, FALSE, null_mut()) };
    if event.is_null() {
        let error = unsafe { GetLastError() };
        unsafe { CloseHandle(handle) };
        return Err(format!("create QGA I/O event: {error}"));
    }

    let result = (|| {
        let request = format!("{GET_MEMORY_STATS_REQUEST}\n");
        overlapped_write(
            handle,
            event,
            request.as_bytes(),
            remaining_timeout_ms(started, timeout),
        )?;

        // Completed named-pipe writes are immediately visible to the server.
        // Avoid synchronous FlushFileBuffers, which cannot be cancelled and
        // can wait indefinitely for the server to consume the bytes.
        let mut response = Vec::new();
        loop {
            let mut chunk = [0u8; 4096];
            let count = overlapped_read(
                handle,
                event,
                &mut chunk,
                remaining_timeout_ms(started, timeout),
            )?;
            if count == 0 {
                break;
            }
            response.extend_from_slice(&chunk[..count]);
            if response.contains(&b'\n') || response.len() > 1024 * 1024 {
                break;
            }
        }

        let response = String::from_utf8(response)
            .map_err(|error| format!("QEMU Guest Agent response is not UTF-8: {error}"))?;
        if response.trim().is_empty() {
            return Err("QEMU Guest Agent returned an empty response".to_owned());
        }
        if !response.contains('\n') {
            return Err("QEMU Guest Agent response exceeded the read boundary".to_owned());
        }
        Ok(response)
    })();

    unsafe {
        CloseHandle(event);
        CloseHandle(handle);
    }
    result
}

#[cfg(windows)]
fn duration_to_timeout_ms(timeout: Duration) -> u32 {
    timeout
        .as_millis()
        .clamp(1, u32::MAX as u128)
        .try_into()
        .unwrap_or(u32::MAX)
}

#[cfg(windows)]
fn remaining_timeout_ms(started: std::time::Instant, timeout: Duration) -> u32 {
    duration_to_timeout_ms(timeout.saturating_sub(started.elapsed()))
}

#[cfg(windows)]
fn overlapped_write(
    handle: winapi::shared::ntdef::HANDLE,
    event: winapi::shared::ntdef::HANDLE,
    bytes: &[u8],
    timeout_ms: u32,
) -> Result<(), String> {
    overlapped_transfer(
        handle,
        event,
        bytes.as_ptr() as *mut _,
        bytes.len(),
        timeout_ms,
        false,
    )
    .map(|_| ())
}

#[cfg(windows)]
fn overlapped_read(
    handle: winapi::shared::ntdef::HANDLE,
    event: winapi::shared::ntdef::HANDLE,
    bytes: &mut [u8],
    timeout_ms: u32,
) -> Result<usize, String> {
    overlapped_transfer(
        handle,
        event,
        bytes.as_mut_ptr() as *mut _,
        bytes.len(),
        timeout_ms,
        true,
    )
}

#[cfg(windows)]
fn overlapped_transfer(
    handle: winapi::shared::ntdef::HANDLE,
    event: winapi::shared::ntdef::HANDLE,
    buffer: *mut winapi::ctypes::c_void,
    length: usize,
    timeout_ms: u32,
    read: bool,
) -> Result<usize, String> {
    use std::mem::zeroed;

    use winapi::shared::minwindef::{DWORD, FALSE};
    use winapi::shared::winerror::{ERROR_BROKEN_PIPE, ERROR_IO_PENDING, WAIT_TIMEOUT};
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::fileapi::{ReadFile, WriteFile};
    use winapi::um::ioapiset::{CancelIoEx, GetOverlappedResult};
    use winapi::um::minwinbase::OVERLAPPED;
    use winapi::um::synchapi::WaitForSingleObject;
    use winapi::um::winbase::WAIT_OBJECT_0;

    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    overlapped.hEvent = event;
    let mut transferred = 0 as DWORD;
    let completed = unsafe {
        if read {
            ReadFile(
                handle,
                buffer,
                length as DWORD,
                &mut transferred,
                &mut overlapped,
            )
        } else {
            WriteFile(
                handle,
                buffer as *const _,
                length as DWORD,
                &mut transferred,
                &mut overlapped,
            )
        }
    };

    if completed == FALSE {
        let error = unsafe { GetLastError() };
        if error != ERROR_IO_PENDING {
            return Err(format!(
                "QGA {}: {error}",
                if read { "read" } else { "write" }
            ));
        }
        let wait = unsafe { WaitForSingleObject(event, timeout_ms) };
        if wait == WAIT_TIMEOUT {
            unsafe { CancelIoEx(handle, &mut overlapped) };
            return Err("QEMU Guest Agent operation deadline exceeded".to_owned());
        }
        if wait != WAIT_OBJECT_0 {
            return Err(format!(
                "wait for QGA {}: {wait}",
                if read { "read" } else { "write" }
            ));
        }
        if unsafe { GetOverlappedResult(handle, &mut overlapped, &mut transferred, 0) } == FALSE {
            let error = unsafe { GetLastError() };
            if error == ERROR_BROKEN_PIPE {
                return Err("QEMU Guest Agent pipe closed during I/O".to_owned());
            }
            return Err(format!(
                "complete QGA {}: {error}",
                if read { "read" } else { "write" }
            ));
        }
    }

    Ok(transferred as usize)
}

#[cfg(not(windows))]
fn request_memory_stats(pipe_path: &str, _timeout: Duration) -> Result<String, String> {
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};

    let mut pipe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(pipe_path)
        .map_err(|error| format!("open {pipe_path}: {error}"))?;
    writeln!(pipe, "{GET_MEMORY_STATS_REQUEST}")
        .map_err(|error| format!("write request: {error}"))?;
    pipe.flush()
        .map_err(|error| format!("flush request: {error}"))?;
    let mut response = String::new();
    BufReader::new(pipe)
        .read_line(&mut response)
        .map_err(|error| format!("read response: {error}"))?;
    if response.trim().is_empty() {
        return Err("QEMU Guest Agent returned an empty response".to_owned());
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_configured_pipe_path() {
        let agent = NamedPipeGuestAgent::new(r"\\.\pipe\qga-test");

        assert_eq!(agent.pipe_path, r"\\.\pipe\qga-test");
        assert_eq!(agent.operation_timeout, DEFAULT_QGA_OPERATION_TIMEOUT);
    }

    #[test]
    fn rejects_zero_operation_timeout_before_starting_io() {
        let mut agent =
            NamedPipeGuestAgent::with_operation_timeout(r"\\.\pipe\qga-test", Duration::ZERO);

        assert_eq!(
            agent.get_memory_stats(),
            Err("QEMU Guest Agent operation timeout must be greater than zero".to_owned())
        );
    }

    #[cfg(windows)]
    #[test]
    fn waits_only_for_win32_named_pipe_paths() {
        assert!(r"\\.\pipe\qga-test"
            .to_ascii_lowercase()
            .starts_with(r"\\.\pipe\"));
        assert!(!r"\\.\Global\org.qemu.guest_agent.0"
            .to_ascii_lowercase()
            .starts_with(r"\\.\pipe\"));
    }

    #[cfg(windows)]
    #[test]
    fn clamps_timeout_to_win32_millisecond_range() {
        assert_eq!(duration_to_timeout_ms(Duration::from_millis(0)), 1);
        assert_eq!(duration_to_timeout_ms(Duration::from_millis(25)), 25);
        assert_eq!(
            duration_to_timeout_ms(Duration::from_secs(u64::MAX)),
            u32::MAX
        );
    }
}
