use crate::service_host::StopSignal;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_RAW_TELEMETRY_RECORD_BYTES: usize = 64 * 1024;
pub const RAW_TELEMETRY_RETENTION_FILES: usize = 3;
#[cfg(any(windows, test))]
const ATOMIC_REPLACE_RETRY_DELAY: Duration = Duration::from_millis(100);

pub use virtio_mem_core::{
    DemandError, MemoryResourceNotificationState, MemoryTelemetrySnapshot, OptionalTelemetrySignal,
    PagingActivitySnapshot, RawTelemetryContractMode, RawTelemetryEnvelope, ReusableMemorySnapshot,
    TelemetryCapability, TelemetrySignalStatus, TelemetryWarmup, WindowsNativeTelemetry,
    WindowsTelemetryCapabilities,
};

/// Publishes raw telemetry without accepting allocation or resize input.
pub trait RawTelemetryPublisher {
    fn publish(&mut self, envelope: &RawTelemetryEnvelope) -> Result<(), String>;

    fn publish_cancellable(
        &mut self,
        envelope: &RawTelemetryEnvelope,
        stop: &StopSignal,
    ) -> Result<(), String> {
        if stop.is_cancelled() {
            return Ok(());
        }
        self.publish(envelope)
    }
}

/// Publishes one complete current record through an atomic replace and keeps a
/// bounded set of previous records for diagnostics. Readers never consume a
/// partially written current record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicRawTelemetryPublisher {
    path: PathBuf,
    retention_files: usize,
    retry_timeout: Duration,
}

impl AtomicRawTelemetryPublisher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            retention_files: RAW_TELEMETRY_RETENTION_FILES,
            retry_timeout: Duration::from_secs(20),
        }
    }

    pub fn with_retention(path: impl Into<PathBuf>, retention_files: usize) -> Self {
        Self {
            path: path.into(),
            retention_files,
            retry_timeout: Duration::from_secs(20),
        }
    }

    pub fn with_retry_timeout(mut self, timeout: Duration) -> Self {
        self.retry_timeout = timeout;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn backup_path(&self, index: usize) -> PathBuf {
        PathBuf::from(format!("{}.{}", self.path.display(), index))
    }
}

impl RawTelemetryPublisher for AtomicRawTelemetryPublisher {
    fn publish(&mut self, envelope: &RawTelemetryEnvelope) -> Result<(), String> {
        self.publish_cancellable(envelope, &StopSignal::new())
    }

    fn publish_cancellable(
        &mut self,
        envelope: &RawTelemetryEnvelope,
        stop: &StopSignal,
    ) -> Result<(), String> {
        if stop.is_cancelled() {
            return Ok(());
        }
        let mut encoded = serde_json::to_vec(envelope)
            .map_err(|error| format!("encode raw telemetry: {error}"))?;
        encoded.push(b'\n');
        if encoded.len() > MAX_RAW_TELEMETRY_RECORD_BYTES {
            return Err(format!(
                "raw telemetry record exceeds {MAX_RAW_TELEMETRY_RECORD_BYTES} byte limit"
            ));
        }
        let retained = (|| -> Result<(), String> {
            if self.retention_files > 0 {
                let oldest = self.backup_path(self.retention_files);
                match std::fs::remove_file(&oldest) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(format!("remove oldest raw telemetry backup: {error}"))
                    }
                }
                for index in (1..self.retention_files).rev() {
                    let from = self.backup_path(index);
                    let to = self.backup_path(index + 1);
                    match std::fs::rename(&from, &to) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(format!("rotate raw telemetry backup: {error}")),
                    }
                }
                if self.path.exists() {
                    std::fs::copy(&self.path, self.backup_path(1)).map_err(|error| {
                        format!("retain previous raw telemetry record: {error}")
                    })?;
                }
            }

            Ok(())
        })();
        if let Err(error) = retained {
            eprintln!("diagnostic telemetry retention failed: {error}");
        }
        if stop.is_cancelled() {
            return Ok(());
        }

        replace_file(&self.path, &encoded, stop, self.retry_timeout)
            .map_err(|error| format!("publish raw telemetry handoff: {error}"))
    }
}

/// Replace `path` with `bytes` through a flushed temporary file and an atomic
/// write-through rename, so readers and restarts see the old or new contents
/// whole. Cancellation abandons the replacement and reports success; the
/// caller's own cancellable publication then publishes nothing either.
pub(crate) fn replace_file(
    path: &Path,
    bytes: &[u8],
    stop: &StopSignal,
    retry_timeout: Duration,
) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let temporary = PathBuf::from(format!("{}.tmp-{}", path.display(), std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("open handoff: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("write handoff: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("flush handoff: {error}"))?;
        drop(file);
        atomic_replace(&temporary, path, stop, retry_timeout)
            .map_err(|error| format!("replace {}: {error}", path.display()))
    })();
    if result.is_err() || stop.is_cancelled() {
        // Best effort: the replacement error, if any, is what is reported.
        let _ = std::fs::remove_file(&temporary);
    }
    if stop.is_cancelled() {
        Ok(())
    } else {
        result
    }
}

#[cfg(not(windows))]
fn atomic_replace(
    source: &Path,
    destination: &Path,
    stop: &StopSignal,
    _: Duration,
) -> std::io::Result<()> {
    if stop.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "publication cancelled",
        ));
    }
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(
    source: &Path,
    destination: &Path,
    stop: &StopSignal,
    timeout: Duration,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use winapi::shared::minwindef::FALSE;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    retry_atomic_replace(stop, timeout, || {
        // SAFETY: Both pointers reference NUL-terminated UTF-16 buffers that
        // live for the complete call, and MoveFileExW does not retain them.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == FALSE
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
}

#[cfg(any(windows, test))]
fn retry_atomic_replace(
    stop: &StopSignal,
    timeout: Duration,
    mut operation: impl FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "publication timeout too large",
        )
    })?;
    loop {
        if stop.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "publication cancelled",
            ));
        }
        match operation() {
            Ok(()) => return Ok(()),
            // ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION are transient.
            // So is ERROR_ACCESS_DENIED while a reader holds the destination
            // open without FILE_SHARE_DELETE, as the QEMU guest agent does
            // for every host read; MoveFileExW reports that collision as
            // access denied. A persistent ACL fault still fails at the
            // deadline.
            Err(error)
                if matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && Instant::now() < deadline =>
            {
                stop.wait(
                    ATOMIC_REPLACE_RETRY_DELAY
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) => return Err(error),
        }
    }
}

pub trait TelemetryClock {
    fn now_unix_millis(&self) -> Result<u64, String>;
    fn monotonic_millis(&self) -> Result<u64, String>;
}

#[derive(Debug, Clone)]
pub struct SystemTelemetryClock {
    started_at: Instant,
}

impl Default for SystemTelemetryClock {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl TelemetryClock for SystemTelemetryClock {
    fn now_unix_millis(&self) -> Result<u64, String> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
        u64::try_from(duration.as_millis())
            .map_err(|_| "system Unix timestamp does not fit in u64 milliseconds".to_owned())
    }

    fn monotonic_millis(&self) -> Result<u64, String> {
        u64::try_from(self.started_at.elapsed().as_millis())
            .map_err(|_| "process monotonic timestamp does not fit in u64 milliseconds".to_owned())
    }
}

/// A per-process identity for this service run's publications.
pub fn process_session_id() -> Result<String, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
    Ok(format!(
        "session-{:x}-{:x}",
        duration.as_nanos(),
        std::process::id()
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryResourceNotificationKind {
    Low,
    High,
}

/// Injectable boundary around the paired Windows memory-resource handles.
pub trait MemoryResourceNotificationApi {
    type Handle: Copy;

    fn create(&self, kind: MemoryResourceNotificationKind) -> Result<Self::Handle, u32>;
    fn query(&self, handle: Self::Handle) -> Result<bool, u32>;
    fn close(&self, handle: Self::Handle);

    /// True means the low-memory object woke the worker. Default is a timer.
    fn wait_for_pressure(
        &self,
        _handle: Option<Self::Handle>,
        stop: &StopSignal,
        timeout: Duration,
        _wake: Option<usize>,
    ) -> Result<bool, u32> {
        stop.wait(timeout);
        Ok(false)
    }
}

pub trait MemoryResourceNotificationTelemetry {
    /// Remain timer-driven once Low is observed, since notifications are level-triggered.
    fn wait(
        &mut self,
        stop: &StopSignal,
        timeout: Duration,
        _watch_low: bool,
        _wake: Option<usize>,
    ) {
        stop.wait(timeout);
    }

    fn capability(&self) -> TelemetryCapability;
    fn collect(
        &self,
        observed_monotonic_millis: u64,
    ) -> OptionalTelemetrySignal<MemoryResourceNotificationState>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableMemoryResourceNotifications;

impl MemoryResourceNotificationTelemetry for UnavailableMemoryResourceNotifications {
    fn capability(&self) -> TelemetryCapability {
        TelemetryCapability::Unavailable
    }

    fn collect(
        &self,
        observed_monotonic_millis: u64,
    ) -> OptionalTelemetrySignal<MemoryResourceNotificationState> {
        OptionalTelemetrySignal::unavailable(observed_monotonic_millis)
    }
}

#[derive(Debug, Clone, Copy)]
enum MemoryResourceNotificationHandles<H> {
    Ready { low: H, high: H },
    CreationFailed(u32),
}

/// Owns both notification handles for one service worker lifetime.
#[derive(Debug)]
pub struct MemoryResourceNotificationCollector<A: MemoryResourceNotificationApi> {
    api: A,
    handles: MemoryResourceNotificationHandles<A::Handle>,
}

impl<A: MemoryResourceNotificationApi> MemoryResourceNotificationCollector<A> {
    pub fn new(api: A) -> Self {
        let low = match api.create(MemoryResourceNotificationKind::Low) {
            Ok(low) => low,
            Err(error_code) => {
                return Self {
                    api,
                    handles: MemoryResourceNotificationHandles::CreationFailed(
                        normalize_error_code(error_code),
                    ),
                };
            }
        };
        let high = match api.create(MemoryResourceNotificationKind::High) {
            Ok(high) => high,
            Err(error_code) => {
                api.close(low);
                return Self {
                    api,
                    handles: MemoryResourceNotificationHandles::CreationFailed(
                        normalize_error_code(error_code),
                    ),
                };
            }
        };
        Self {
            api,
            handles: MemoryResourceNotificationHandles::Ready { low, high },
        }
    }
}

impl<A: MemoryResourceNotificationApi> MemoryResourceNotificationTelemetry
    for MemoryResourceNotificationCollector<A>
{
    fn wait(&mut self, stop: &StopSignal, timeout: Duration, watch_low: bool, wake: Option<usize>) {
        if stop.is_cancelled() {
            return;
        }
        if matches!(
            self.handles,
            MemoryResourceNotificationHandles::CreationFailed(_)
        ) {
            // A missing optional facility must not permanently disable observation.
            match self.api.create(MemoryResourceNotificationKind::Low) {
                Ok(low) => match self.api.create(MemoryResourceNotificationKind::High) {
                    Ok(high) => {
                        self.handles = MemoryResourceNotificationHandles::Ready { low, high }
                    }
                    Err(_) => self.api.close(low),
                },
                Err(_) => { /* Retry at the next bounded poll. */ }
            }
        }
        let low = match self.handles {
            MemoryResourceNotificationHandles::Ready { low, .. } if watch_low => Some(low),
            _ => None,
        };
        match self.api.wait_for_pressure(low, stop, timeout, wake) {
            Ok(_) => {}
            Err(error) => {
                eprintln!("native memory notification wait failed: {error}");
                stop.wait(timeout);
            }
        }
    }

    fn capability(&self) -> TelemetryCapability {
        TelemetryCapability::Supported
    }

    fn collect(
        &self,
        observed_monotonic_millis: u64,
    ) -> OptionalTelemetrySignal<MemoryResourceNotificationState> {
        let (low, high) = match self.handles {
            MemoryResourceNotificationHandles::Ready { low, high } => (low, high),
            MemoryResourceNotificationHandles::CreationFailed(error_code) => {
                return OptionalTelemetrySignal::failed(observed_monotonic_millis, error_code);
            }
        };
        let low = match self.api.query(low) {
            Ok(value) => value,
            Err(error_code) => {
                return OptionalTelemetrySignal::failed(
                    observed_monotonic_millis,
                    normalize_error_code(error_code),
                );
            }
        };
        let high = match self.api.query(high) {
            Ok(value) => value,
            Err(error_code) => {
                return OptionalTelemetrySignal::failed(
                    observed_monotonic_millis,
                    normalize_error_code(error_code),
                );
            }
        };
        let state = match (low, high) {
            (true, false) => MemoryResourceNotificationState::Low,
            (false, true) => MemoryResourceNotificationState::High,
            (false, false) => MemoryResourceNotificationState::Neutral,
            (true, true) => {
                return OptionalTelemetrySignal::failed(
                    observed_monotonic_millis,
                    CONTRADICTORY_NOTIFICATION_ERROR_CODE,
                );
            }
        };
        OptionalTelemetrySignal::supported(observed_monotonic_millis, state)
    }
}

impl<A: MemoryResourceNotificationApi> Drop for MemoryResourceNotificationCollector<A> {
    fn drop(&mut self) {
        if let MemoryResourceNotificationHandles::Ready { low, high } = self.handles {
            self.api.close(low);
            self.api.close(high);
        }
    }
}

const FALLBACK_NOTIFICATION_ERROR_CODE: u32 = 31;
const CONTRADICTORY_NOTIFICATION_ERROR_CODE: u32 = 13;

fn normalize_error_code(error_code: u32) -> u32 {
    if error_code == 0 {
        FALLBACK_NOTIFICATION_ERROR_CODE
    } else {
        error_code
    }
}

#[cfg(windows)]
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsMemoryResourceNotificationApi;

#[cfg(windows)]
impl MemoryResourceNotificationApi for WindowsMemoryResourceNotificationApi {
    type Handle = usize;

    fn create(&self, kind: MemoryResourceNotificationKind) -> Result<Self::Handle, u32> {
        use winapi::um::memoryapi::{
            CreateMemoryResourceNotification, HighMemoryResourceNotification,
            LowMemoryResourceNotification,
        };

        let notification_type = match kind {
            MemoryResourceNotificationKind::Low => LowMemoryResourceNotification,
            MemoryResourceNotificationKind::High => HighMemoryResourceNotification,
        };
        // SAFETY: The API receives a valid documented enum and returns an owned
        // handle or null. The collector closes every successfully created handle.
        let handle = unsafe { CreateMemoryResourceNotification(notification_type) };
        if handle.is_null() {
            Err(last_windows_error_code())
        } else {
            Ok(handle as usize)
        }
    }

    fn query(&self, handle: Self::Handle) -> Result<bool, u32> {
        use winapi::ctypes::c_void;
        use winapi::shared::minwindef::FALSE;
        use winapi::um::memoryapi::QueryMemoryResourceNotification;

        let mut signaled = FALSE;
        // SAFETY: `handle` was returned by the create call and remains owned by
        // the collector; the output pointer is valid for the duration of the call.
        let result =
            unsafe { QueryMemoryResourceNotification(handle as *mut c_void, &mut signaled) };
        if result == FALSE {
            Err(last_windows_error_code())
        } else {
            Ok(signaled != FALSE)
        }
    }

    fn wait_for_pressure(
        &self,
        low: Option<Self::Handle>,
        stop: &StopSignal,
        timeout: Duration,
        wake: Option<usize>,
    ) -> Result<bool, u32> {
        use winapi::shared::ntdef::HANDLE;
        use winapi::shared::winerror::WAIT_TIMEOUT;
        use winapi::um::synchapi::WaitForMultipleObjects;
        use winapi::um::winbase::WAIT_OBJECT_0;
        let mut handles = Vec::new();
        if let Some(event) = stop.native_event() {
            handles.push(event as HANDLE);
        }
        if let Some(event) = low {
            handles.push(event as HANDLE);
        }
        if let Some(event) = wake {
            handles.push(event as HANDLE);
        }
        if handles.is_empty() {
            stop.wait(timeout);
            return Ok(false);
        }
        let started = Instant::now();
        while !stop.is_cancelled() && started.elapsed() < timeout {
            let remaining = timeout.saturating_sub(started.elapsed());
            let bound = if stop.native_event().is_some() {
                remaining
            } else {
                remaining.min(Duration::from_millis(50))
            };
            let millis = bound.as_millis().max(1).min(u128::from(u32::MAX - 1)) as u32;
            // SAFETY: each handle is owned by the collector, stop signal or
            // wake channel, all borrowed for this wait and alive until it ends.
            let result = unsafe {
                WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, millis)
            };
            if result < WAIT_OBJECT_0 + handles.len() as u32 {
                return Ok(!stop.is_cancelled());
            }
            if result != WAIT_TIMEOUT {
                return Err(last_windows_error_code());
            }
        }
        Ok(false)
    }

    fn close(&self, handle: Self::Handle) {
        use winapi::ctypes::c_void;
        use winapi::um::handleapi::CloseHandle;

        // SAFETY: The collector calls this exactly once for each owned handle.
        let _ = unsafe { CloseHandle(handle as *mut c_void) };
    }
}

#[cfg(windows)]
fn last_windows_error_code() -> u32 {
    normalize_error_code(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default() as u32,
    )
}

#[cfg(windows)]
pub fn native_memory_resource_notifications(
) -> MemoryResourceNotificationCollector<WindowsMemoryResourceNotificationApi> {
    MemoryResourceNotificationCollector::new(WindowsMemoryResourceNotificationApi)
}

/// Collects native Windows memory counters using the documented system APIs.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeMemoryTelemetry;

fn pages_to_bytes_checked(pages: u64, page_size: u64) -> Result<u64, DemandError> {
    pages
        .checked_mul(page_size)
        .ok_or(DemandError::ArithmeticOverflow)
}

pub trait MemoryTelemetry {
    fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError>;
}

impl MemoryTelemetry for NativeMemoryTelemetry {
    #[cfg(windows)]
    fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
        use std::mem::{size_of, zeroed};
        use winapi::shared::minwindef::FALSE;
        use winapi::um::psapi::{GetPerformanceInfo, PERFORMANCE_INFORMATION};

        // SAFETY: The Windows API initializes the supplied structure and only
        // writes within its declared size.
        let performance = unsafe {
            let mut value: PERFORMANCE_INFORMATION = zeroed();
            if GetPerformanceInfo(&mut value, size_of::<PERFORMANCE_INFORMATION>() as u32) == FALSE
            {
                return Err(DemandError::PerformanceInfo(
                    std::io::Error::last_os_error()
                        .raw_os_error()
                        .unwrap_or_default() as u32,
                ));
            }
            value
        };
        let page_size = performance.PageSize as u64;
        if page_size == 0 {
            return Err(DemandError::ZeroCounter("page size"));
        }
        let pages_to_bytes = |pages: usize| pages_to_bytes_checked(pages as u64, page_size);
        let total = pages_to_bytes(performance.PhysicalTotal)?;
        let available = pages_to_bytes(performance.PhysicalAvailable)?;
        if total == 0 || available > total {
            return Err(DemandError::InconsistentCounters(
                "invalid physical memory snapshot",
            ));
        }

        let snapshot = MemoryTelemetrySnapshot {
            physical_total_bytes: total,
            physical_available_bytes: available,
            memory_load_percent: ((u128::from(total - available) * 100) / u128::from(total)) as u32,
            commit_total_bytes: pages_to_bytes(performance.CommitTotal)?,
            commit_limit_bytes: pages_to_bytes(performance.CommitLimit)?,
            commit_peak_bytes: pages_to_bytes(performance.CommitPeak)?,
            system_cache_bytes: pages_to_bytes(performance.SystemCache)?,
            kernel_paged_bytes: pages_to_bytes(performance.KernelPaged)?,
            kernel_nonpaged_bytes: pages_to_bytes(performance.KernelNonpaged)?,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    #[cfg(not(windows))]
    fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
        Err(DemandError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn snapshot(available: u64, commit: u64) -> MemoryTelemetrySnapshot {
        MemoryTelemetrySnapshot {
            physical_total_bytes: 16 * GIB,
            physical_available_bytes: available,
            memory_load_percent: 50,
            commit_total_bytes: commit,
            commit_limit_bytes: 16 * GIB,
            commit_peak_bytes: commit,
            system_cache_bytes: 0,
            kernel_paged_bytes: 0,
            kernel_nonpaged_bytes: 0,
        }
    }

    #[derive(Debug, Default)]
    struct NotificationFixtureState {
        creates: VecDeque<Result<u8, u32>>,
        low_queries: VecDeque<Result<bool, u32>>,
        high_queries: VecDeque<Result<bool, u32>>,
        closed: Vec<u8>,
    }

    #[derive(Debug, Clone)]
    struct NotificationFixtureApi(Arc<Mutex<NotificationFixtureState>>);

    impl MemoryResourceNotificationApi for NotificationFixtureApi {
        type Handle = u8;

        fn create(&self, _kind: MemoryResourceNotificationKind) -> Result<Self::Handle, u32> {
            self.0
                .lock()
                .expect("notification fixture lock")
                .creates
                .pop_front()
                .expect("scripted create result")
        }

        fn query(&self, handle: Self::Handle) -> Result<bool, u32> {
            let mut state = self.0.lock().expect("notification fixture lock");
            let results = if handle == 1 {
                &mut state.low_queries
            } else {
                &mut state.high_queries
            };
            results.pop_front().expect("scripted query result")
        }

        fn close(&self, handle: Self::Handle) {
            self.0
                .lock()
                .expect("notification fixture lock")
                .closed
                .push(handle);
        }
    }

    fn notification_fixture(
        state: NotificationFixtureState,
    ) -> (NotificationFixtureApi, Arc<Mutex<NotificationFixtureState>>) {
        let state = Arc::new(Mutex::new(state));
        (NotificationFixtureApi(state.clone()), state)
    }

    #[test]
    fn notification_collector_reports_neutral_low_and_high() {
        let (api, state) = notification_fixture(NotificationFixtureState {
            creates: VecDeque::from([Ok(1), Ok(2)]),
            low_queries: VecDeque::from([Ok(false), Ok(true), Ok(false)]),
            high_queries: VecDeque::from([Ok(false), Ok(false), Ok(true)]),
            ..NotificationFixtureState::default()
        });
        let collector = MemoryResourceNotificationCollector::new(api);

        for (monotonic, expected) in [
            (10, MemoryResourceNotificationState::Neutral),
            (20, MemoryResourceNotificationState::Low),
            (30, MemoryResourceNotificationState::High),
        ] {
            assert_eq!(
                collector.collect(monotonic),
                OptionalTelemetrySignal::supported(monotonic, expected)
            );
        }
        drop(collector);
        assert_eq!(
            state.lock().expect("notification fixture lock").closed,
            vec![1, 2]
        );
    }

    #[test]
    fn notification_creation_failure_is_reported_and_cleans_partial_state() {
        let (api, state) = notification_fixture(NotificationFixtureState {
            creates: VecDeque::from([Ok(1), Err(5)]),
            ..NotificationFixtureState::default()
        });
        let collector = MemoryResourceNotificationCollector::new(api);

        assert_eq!(collector.capability(), TelemetryCapability::Supported);
        assert_eq!(
            collector.collect(10),
            OptionalTelemetrySignal::failed(10, 5)
        );
        drop(collector);
        assert_eq!(
            state.lock().expect("notification fixture lock").closed,
            vec![1]
        );
    }

    #[test]
    fn notification_query_failure_and_contradiction_fail_closed_then_recover() {
        let (api, _state) = notification_fixture(NotificationFixtureState {
            creates: VecDeque::from([Ok(1), Ok(2)]),
            low_queries: VecDeque::from([Err(0), Ok(true), Ok(false)]),
            high_queries: VecDeque::from([Ok(true), Ok(true)]),
            ..NotificationFixtureState::default()
        });
        let collector = MemoryResourceNotificationCollector::new(api);

        assert_eq!(
            collector.collect(10),
            OptionalTelemetrySignal::failed(10, FALLBACK_NOTIFICATION_ERROR_CODE)
        );
        assert_eq!(
            collector.collect(20),
            OptionalTelemetrySignal::failed(20, CONTRADICTORY_NOTIFICATION_ERROR_CODE)
        );
        assert_eq!(
            collector.collect(30),
            OptionalTelemetrySignal::supported(30, MemoryResourceNotificationState::High)
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_notification_api_reports_a_supported_categorical_state() {
        let collector = native_memory_resource_notifications();
        let observation = collector.collect(1);

        assert_eq!(collector.capability(), TelemetryCapability::Supported);
        assert_eq!(observation.status, TelemetrySignalStatus::Supported);
        assert!(matches!(
            observation.value,
            Some(
                MemoryResourceNotificationState::Low
                    | MemoryResourceNotificationState::Neutral
                    | MemoryResourceNotificationState::High
            )
        ));
    }

    #[test]
    fn rejects_invalid_counters() {
        let mut value = snapshot(8 * GIB, 4 * GIB);
        value.physical_available_bytes = 17 * GIB;
        assert_eq!(
            value.validate(),
            Err(DemandError::InconsistentCounters(
                "physical available exceeds physical total"
            ))
        );

        value = snapshot(8 * GIB, 4 * GIB);
        value.commit_limit_bytes = 0;
        assert_eq!(
            value.validate(),
            Err(DemandError::ZeroCounter("commit limit"))
        );
    }

    #[test]
    fn atomic_publisher_handoffs_complete_records_with_bounded_retention() {
        let path = std::env::temp_dir().join(format!(
            "virtio-mem-raw-telemetry-{}-atomic.jsonl",
            std::process::id()
        ));
        for suffix in ["", ".1", ".2", ".3"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        let mut publisher = AtomicRawTelemetryPublisher::with_retention(&path, 2);
        for sequence in 0..4 {
            let envelope = RawTelemetryEnvelope::new(
                "guest",
                "VirtioMemService",
                "session-a",
                1_000_000 + sequence,
                10 + sequence,
                sequence,
                snapshot(2 * GIB, 15 * GIB),
            );
            publisher.publish(&envelope).expect("atomic publish");
        }
        let current: RawTelemetryEnvelope = serde_json::from_str(
            std::fs::read_to_string(&path)
                .expect("current record")
                .trim(),
        )
        .expect("complete JSON");
        let previous: RawTelemetryEnvelope = serde_json::from_str(
            std::fs::read_to_string(format!("{}.1", path.display()))
                .expect("previous record")
                .trim(),
        )
        .expect("complete retained JSON");
        assert_eq!(current.sequence, 3);
        assert_eq!(previous.sequence, 2);
        assert!(std::fs::metadata(format!("{}.3", path.display())).is_err());
        for suffix in ["", ".1", ".2"] {
            std::fs::remove_file(format!("{}{suffix}", path.display())).expect("remove fixture");
        }
    }

    #[test]
    fn unusable_diagnostic_backup_does_not_block_current_publication() {
        let root =
            std::env::temp_dir().join(format!("virtio-backup-failure-{}", std::process::id()));
        let path = root.join("telemetry.json");
        std::fs::create_dir_all(root.join("telemetry.json.1")).unwrap();
        let envelope = RawTelemetryEnvelope::new(
            "guest",
            "service",
            "session",
            1000,
            10,
            0,
            snapshot(2 * GIB, 15 * GIB),
        );
        let mut publisher = AtomicRawTelemetryPublisher::with_retention(&path, 1);
        publisher.publish(&envelope).unwrap();
        publisher
            .publish(&RawTelemetryEnvelope {
                sequence: 1,
                ..envelope
            })
            .unwrap();
        let current: RawTelemetryEnvelope =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(current.sequence, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_collector_has_platform_behavior() {
        #[cfg(not(windows))]
        assert_eq!(
            NativeMemoryTelemetry.collect(),
            Err(DemandError::UnsupportedPlatform)
        );
    }

    #[test]
    fn publication_retry_is_cancellable_and_persistent_access_denial_fails_at_the_deadline() {
        let stop = StopSignal::new();
        let error = retry_atomic_replace(&stop, Duration::from_secs(1), || {
            stop.cancel();
            Err(std::io::Error::from_raw_os_error(32))
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        let started = Instant::now();
        let mut calls = 0;
        let error = retry_atomic_replace(&StopSignal::new(), Duration::from_millis(250), || {
            calls += 1;
            Err(std::io::Error::from_raw_os_error(5))
        })
        .expect_err("persistent access denial must fail");
        assert_eq!(error.raw_os_error(), Some(5));
        assert!(calls > 1);
        assert!(started.elapsed() >= Duration::from_millis(250));
    }

    #[test]
    fn atomic_replace_retries_only_transient_access_denial() {
        let mut attempts = 0;
        retry_atomic_replace(&StopSignal::new(), Duration::from_secs(1), || {
            attempts += 1;
            if attempts < 3 {
                Err(std::io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        })
        .expect("transient sharing collision should recover");
        assert_eq!(attempts, 3);

        // A guest-agent read holding the destination open surfaces as access
        // denied until it closes the handle.
        let mut attempts = 0;
        retry_atomic_replace(&StopSignal::new(), Duration::from_secs(1), || {
            attempts += 1;
            if attempts < 3 {
                Err(std::io::Error::from_raw_os_error(5))
            } else {
                Ok(())
            }
        })
        .expect("access denial from an open reader should recover");
        assert_eq!(attempts, 3);

        let mut attempts = 0;
        assert_eq!(
            retry_atomic_replace(&StopSignal::new(), Duration::from_secs(1), || {
                attempts += 1;
                Err(std::io::Error::from(std::io::ErrorKind::InvalidInput))
            })
            .expect_err("non-transient error should fail")
            .kind(),
            std::io::ErrorKind::InvalidInput
        );
        assert_eq!(attempts, 1);
    }
    #[test]
    fn checked_page_conversion_rejects_overflow() {
        assert_eq!(pages_to_bytes_checked(3, 4096).unwrap(), 12288);
        assert!(pages_to_bytes_checked(u64::MAX, 4096).is_err());
    }

    #[test]
    fn failed_notification_creation_is_retried_on_a_bounded_wait() {
        let (api, state) = notification_fixture(NotificationFixtureState {
            creates: VecDeque::from([Err(5), Ok(1), Ok(2)]),
            low_queries: VecDeque::from([Ok(false)]),
            high_queries: VecDeque::from([Ok(true)]),
            ..NotificationFixtureState::default()
        });
        let mut collector = MemoryResourceNotificationCollector::new(api);
        assert_eq!(
            collector.collect(1).status,
            virtio_mem_core::TelemetrySignalStatus::Failed
        );
        collector.wait(&StopSignal::new(), Duration::ZERO, false, None);
        assert_eq!(
            collector.collect(2).value,
            Some(MemoryResourceNotificationState::High)
        );
        drop(collector);
        assert_eq!(state.lock().unwrap().closed, vec![1, 2]);
    }

    #[cfg(windows)]
    #[test]
    fn result_notification_wakes_even_when_low_is_not_watched() {
        let api = WindowsMemoryResourceNotificationApi;
        // SAFETY: private signalled event, owned until the bounded wait ends.
        let handle = unsafe {
            winapi::um::synchapi::CreateEventW(std::ptr::null_mut(), 1, 1, std::ptr::null())
        };
        assert!(!handle.is_null());
        let start = Instant::now();
        assert!(api
            .wait_for_pressure(
                None,
                &StopSignal::new(),
                Duration::from_secs(10),
                Some(handle as usize)
            )
            .unwrap());
        api.close(handle as usize);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[cfg(windows)]
    #[test]
    fn native_memory_wait_is_cancelled_promptly() {
        let stop = StopSignal::new();
        let other = stop.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            other.cancel();
        });
        let api = WindowsMemoryResourceNotificationApi;
        // A private nonsignalled event makes this test independent of the
        // machine's actual memory pressure. The wait uses the same kernel path.
        let handle = unsafe {
            winapi::um::synchapi::CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null())
        };
        assert!(!handle.is_null());
        let handle = handle as usize;
        let start = Instant::now();
        api.wait_for_pressure(Some(handle), &stop, Duration::from_secs(10), None)
            .unwrap();
        api.close(handle);
        thread.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
