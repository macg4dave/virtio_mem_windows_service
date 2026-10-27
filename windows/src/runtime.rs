use std::time::Duration;

use virtio_mem_core::TargetSample;

use crate::demand::{
    MemoryResourceNotificationTelemetry, MemoryTelemetry, RawTelemetryEnvelope,
    RawTelemetryPublisher, TelemetryClock,
};
use crate::requests::GuestRequests;
use crate::service_host::{ServiceWorker, StopSignal};
#[cfg(windows)]
use crate::{
    config::ServiceConfig,
    demand::{
        native_memory_resource_notifications, process_session_id, AtomicRawTelemetryPublisher,
        MemoryResourceNotificationCollector, NativeMemoryTelemetry, SystemTelemetryClock,
        WindowsMemoryResourceNotificationApi,
    },
    error::RuntimeWiringError,
};

/// Production worker: samples native memory telemetry, lets the guest
/// policy decide the one add/remove request, and publishes both atomically.
/// It never resizes anything itself; the host executes requests.
#[derive(Debug)]
pub struct GuestMemoryWorker<T, P, C, N> {
    telemetry: T,
    memory_resource_notifications: N,
    publisher: P,
    clock: C,
    requests: GuestRequests,
    next_sequence: u64,
    previous_monotonic_millis: Option<u64>,
    interval: Duration,
}

impl<T, P, C, N> GuestMemoryWorker<T, P, C, N>
where
    T: MemoryTelemetry,
    P: RawTelemetryPublisher,
    C: TelemetryClock,
    N: MemoryResourceNotificationTelemetry,
{
    /// Identity comes from `requests`, which validates it.
    pub fn new(
        telemetry: T,
        memory_resource_notifications: N,
        publisher: P,
        clock: C,
        requests: GuestRequests,
        interval: Duration,
    ) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("polling interval must be greater than zero".to_owned());
        }
        Ok(Self {
            telemetry,
            memory_resource_notifications,
            publisher,
            clock,
            requests,
            next_sequence: 0,
            previous_monotonic_millis: None,
            interval,
        })
    }

    pub fn poll_once(&mut self) -> Result<RawTelemetryEnvelope, String> {
        self.poll_once_with_stop(&StopSignal::new())
    }

    fn poll_once_with_stop(&mut self, stop: &StopSignal) -> Result<RawTelemetryEnvelope, String> {
        let memory = self
            .telemetry
            .collect()
            .map_err(|error| format!("native memory telemetry failed: {error}"))?;
        let observed_unix_millis = self.clock.now_unix_millis()?;
        let monotonic_millis = self.clock.monotonic_millis()?;
        if self
            .previous_monotonic_millis
            .is_some_and(|previous| monotonic_millis <= previous)
        {
            return Err("raw telemetry monotonic clock did not advance".to_owned());
        }
        let requester = self.requests.requester();
        let mut envelope = RawTelemetryEnvelope::new(
            requester.vm_name(),
            requester.service_name(),
            requester.session_id(),
            observed_unix_millis,
            monotonic_millis,
            self.next_sequence,
            memory,
        );
        let windows_native = envelope.windows_native.as_mut().ok_or_else(|| {
            "current raw telemetry is missing its Windows-native extension".to_owned()
        })?;
        windows_native.capabilities.memory_resource_notifications =
            self.memory_resource_notifications.capability();
        windows_native.memory_resource_notifications =
            self.memory_resource_notifications.collect(monotonic_millis);
        envelope
            .contract_mode()
            .map_err(|error| format!("native telemetry contract failed: {error}"))?;
        envelope.request = self
            .requests
            .poll(
                TargetSample {
                    session_id: envelope.session_id.clone(),
                    observed_unix_millis,
                    monotonic_millis,
                    sequence: envelope.sequence,
                    memory,
                },
                stop,
            )
            .map_err(|error| format!("guest memory request failed: {error}"))?;
        self.publisher
            .publish_cancellable(&envelope, stop)
            .map_err(|error| format!("raw telemetry publication failed: {error}"))?;
        self.previous_monotonic_millis = Some(monotonic_millis);
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| "raw telemetry sequence exhausted".to_owned())?;
        Ok(envelope)
    }

    pub fn publisher(&self) -> &P {
        &self.publisher
    }
}

impl<T, P, C, N> ServiceWorker for GuestMemoryWorker<T, P, C, N>
where
    T: MemoryTelemetry + Send + 'static,
    P: RawTelemetryPublisher + Send + 'static,
    C: TelemetryClock + Send + 'static,
    N: MemoryResourceNotificationTelemetry + Send + 'static,
{
    fn initialize(&mut self, stop: &StopSignal) -> Result<(), String> {
        if stop.is_cancelled() {
            return Ok(());
        }
        self.poll_once_with_stop(stop).map(|_| ())
    }

    fn run(&mut self, stop: &StopSignal) -> Result<(), String> {
        while !stop.is_cancelled() {
            stop.wait(self.interval);
            if !stop.is_cancelled() {
                self.poll_once_with_stop(stop)?;
            }
        }
        Ok(())
    }
}

/// The worker the service runs; console and SCM start-up both build it here.
#[cfg(windows)]
pub type NativeWorker = GuestMemoryWorker<
    NativeMemoryTelemetry,
    AtomicRawTelemetryPublisher,
    SystemTelemetryClock,
    MemoryResourceNotificationCollector<WindowsMemoryResourceNotificationApi>,
>;

#[cfg(windows)]
pub fn native_worker(config: &ServiceConfig) -> Result<NativeWorker, RuntimeWiringError> {
    let session_id =
        process_session_id(&config.service_name).map_err(RuntimeWiringError::WorkerConstruction)?;
    let requests = GuestRequests::open(
        config.policy,
        &config.vm_name,
        &config.service_name,
        &session_id,
        &config.demand_report_path,
        config.shutdown_timeout,
    )
    .map_err(RuntimeWiringError::WorkerConstruction)?;
    GuestMemoryWorker::new(
        NativeMemoryTelemetry,
        native_memory_resource_notifications(),
        AtomicRawTelemetryPublisher::new(&config.demand_report_path)
            .with_retry_timeout(config.shutdown_timeout),
        SystemTelemetryClock::default(),
        requests,
        config.poll_interval,
    )
    .map_err(RuntimeWiringError::WorkerConstruction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demand::{
        DemandError, MemoryResourceNotificationState, MemoryTelemetrySnapshot,
        OptionalTelemetrySignal, RawTelemetryContractMode, TelemetryCapability,
        UnavailableMemoryResourceNotifications,
    };
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use virtio_mem_core::{HostMemoryStatus, MemoryOperation, MEMORY_REQUEST_VERSION};

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    #[derive(Clone)]
    struct TelemetryFixture;

    impl MemoryTelemetry for TelemetryFixture {
        fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
            Ok(MemoryTelemetrySnapshot {
                physical_total_bytes: 16 * GIB,
                physical_available_bytes: 2 * GIB,
                memory_load_percent: 88,
                commit_total_bytes: 15 * GIB,
                commit_limit_bytes: 16 * GIB,
                commit_peak_bytes: 15 * GIB,
                system_cache_bytes: 0,
                kernel_paged_bytes: 0,
                kernel_nonpaged_bytes: 0,
            })
        }
    }

    struct FixedNotifications {
        state: MemoryResourceNotificationState,
    }

    impl MemoryResourceNotificationTelemetry for FixedNotifications {
        fn capability(&self) -> TelemetryCapability {
            TelemetryCapability::Supported
        }

        fn collect(
            &self,
            observed_monotonic_millis: u64,
        ) -> OptionalTelemetrySignal<MemoryResourceNotificationState> {
            OptionalTelemetrySignal::supported(observed_monotonic_millis, self.state)
        }
    }

    struct DropTrackedNotifications(Arc<AtomicBool>);

    impl MemoryResourceNotificationTelemetry for DropTrackedNotifications {
        fn capability(&self) -> TelemetryCapability {
            TelemetryCapability::Supported
        }

        fn collect(
            &self,
            observed_monotonic_millis: u64,
        ) -> OptionalTelemetrySignal<MemoryResourceNotificationState> {
            OptionalTelemetrySignal::supported(
                observed_monotonic_millis,
                MemoryResourceNotificationState::Neutral,
            )
        }
    }

    impl Drop for DropTrackedNotifications {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    /// Advances one second per sample.
    #[derive(Debug, Default)]
    struct StepClock(Cell<u64>);

    impl TelemetryClock for StepClock {
        fn now_unix_millis(&self) -> Result<u64, String> {
            Ok(1_000_000 + self.0.get() * 1000)
        }

        fn monotonic_millis(&self) -> Result<u64, String> {
            self.0.set(self.0.get() + 1);
            Ok(self.0.get() * 1000)
        }
    }

    #[derive(Default, Debug)]
    struct PublisherFixture(Vec<RawTelemetryEnvelope>);

    impl RawTelemetryPublisher for PublisherFixture {
        fn publish(&mut self, envelope: &RawTelemetryEnvelope) -> Result<(), String> {
            self.0.push(envelope.clone());
            Ok(())
        }
    }

    fn publication(name: &str) -> String {
        let path: PathBuf = std::env::temp_dir().join(format!(
            "virtio-mem-worker-{name}-{}.json",
            std::process::id()
        ));
        let path = path.display().to_string();
        for file in [
            virtio_mem_core::host_status_path(&path),
            virtio_mem_core::request_state_path(&path),
        ] {
            // A previous run may have left fixtures behind.
            let _ = std::fs::remove_file(file);
        }
        path
    }

    fn requests(publication: &str, session: &str) -> GuestRequests {
        GuestRequests::open(
            crate::config::test_config().policy,
            "guest",
            "TelemetryService",
            session,
            publication,
            Duration::from_secs(1),
        )
        .expect("request state")
    }

    fn worker<N: MemoryResourceNotificationTelemetry>(
        notifications: N,
        requests: GuestRequests,
    ) -> GuestMemoryWorker<TelemetryFixture, PublisherFixture, StepClock, N> {
        GuestMemoryWorker::new(
            TelemetryFixture,
            notifications,
            PublisherFixture::default(),
            StepClock::default(),
            requests,
            Duration::from_secs(1),
        )
        .expect("worker should be valid")
    }

    #[test]
    fn cancelled_initialization_does_not_sample_or_publish() {
        let path = publication("cancelled");
        let mut worker = worker(
            UnavailableMemoryResourceNotifications,
            requests(&path, "session"),
        );
        let stop = StopSignal::new();
        stop.cancel();
        worker.initialize(&stop).unwrap();
        assert!(worker.publisher().0.is_empty());
    }

    #[test]
    fn without_host_status_the_worker_publishes_telemetry_and_no_request() {
        let path = publication("no-status");
        let mut worker = worker(
            UnavailableMemoryResourceNotifications,
            requests(&path, "session-a"),
        );
        let envelope = worker.poll_once().expect("publication should pass");
        assert_eq!(envelope.vm_name, "guest");
        assert_eq!(envelope.sequence, 0);
        assert_eq!(envelope.request, None);
        assert_eq!(
            envelope.contract_mode(),
            Ok(RawTelemetryContractMode::WindowsNativeFallback),
            "a worker without a native notification collector publishes explicit fallback"
        );
        assert_eq!(worker.publisher().0, vec![envelope]);
    }

    #[test]
    fn a_request_is_durable_before_publication_and_survives_restart() {
        let path = publication("request");
        // The fixture leaves 1 GiB less commit headroom than the reserve.
        let status = HostMemoryStatus {
            version: MEMORY_REQUEST_VERSION,
            vm_name: "guest".to_owned(),
            service_name: "TelemetryService".to_owned(),
            device_alias: "memory0".to_owned(),
            observed_unix_millis: 1_000_000,
            device_size_bytes: 32 * GIB,
            block_size_bytes: 2 * MIB,
            requested_bytes: 4 * GIB,
            current_bytes: 4 * GIB,
            result: None,
        };
        let mut bytes = serde_json::to_vec(&status).unwrap();
        bytes.push(b'\n');
        std::fs::write(virtio_mem_core::host_status_path(&path), bytes).unwrap();

        let mut worker = worker(
            UnavailableMemoryResourceNotifications,
            requests(&path, "session-a"),
        );
        let request = worker
            .poll_once()
            .unwrap()
            .request
            .expect("pressure issues a request");
        assert_eq!(request.operation, MemoryOperation::Add);
        assert_eq!(request.bytes, GIB);
        let retained: virtio_mem_core::GuestRequestState = serde_json::from_slice(
            &std::fs::read(virtio_mem_core::request_state_path(&path)).unwrap(),
        )
        .unwrap();
        assert_eq!(retained.pending.as_ref(), Some(&request));

        let mut restarted = worker_after_restart(&path);
        assert_eq!(restarted.poll_once().unwrap().request, Some(request));
    }

    fn worker_after_restart(
        path: &str,
    ) -> GuestMemoryWorker<
        TelemetryFixture,
        PublisherFixture,
        StepClock,
        UnavailableMemoryResourceNotifications,
    > {
        worker(
            UnavailableMemoryResourceNotifications,
            requests(path, "session-b"),
        )
    }

    #[test]
    fn publishes_native_notification_state_uninterpreted() {
        let path = publication("notifications");
        let mut worker = worker(
            FixedNotifications {
                state: MemoryResourceNotificationState::Low,
            },
            requests(&path, "session-a"),
        );
        let envelope = worker.poll_once().expect("publication should pass");
        assert_eq!(
            envelope.contract_mode(),
            Ok(RawTelemetryContractMode::WindowsNativeSignals)
        );
        assert_eq!(
            envelope
                .windows_native
                .expect("Windows-native extension")
                .memory_resource_notifications
                .value,
            Some(MemoryResourceNotificationState::Low)
        );
    }

    #[cfg(windows)]
    #[test]
    fn publishes_a_real_native_notification_observation() {
        let path = publication("native-notifications");
        let mut worker = worker(
            crate::demand::native_memory_resource_notifications(),
            requests(&path, "session-a"),
        );
        let envelope = worker
            .poll_once()
            .expect("native observation should publish");
        assert_eq!(
            envelope.contract_mode(),
            Ok(RawTelemetryContractMode::WindowsNativeSignals)
        );
        assert_eq!(worker.publisher().0, vec![envelope]);
    }

    #[test]
    fn cancellation_drops_notification_ownership_without_an_extra_poll() {
        let path = publication("drop");
        let dropped = Arc::new(AtomicBool::new(false));
        let mut worker = worker(
            DropTrackedNotifications(dropped.clone()),
            requests(&path, "session-a"),
        );
        let stop = StopSignal::new();
        stop.cancel();

        assert_eq!(worker.run(&stop), Ok(()));
        assert!(worker.publisher().0.is_empty());
        drop(worker);
        assert!(dropped.load(Ordering::Acquire));
    }

    #[test]
    fn rejects_zero_interval_and_invalid_identity() {
        let path = publication("invalid");
        assert!(GuestMemoryWorker::new(
            TelemetryFixture,
            UnavailableMemoryResourceNotifications,
            PublisherFixture::default(),
            StepClock::default(),
            requests(&path, "session-a"),
            Duration::ZERO,
        )
        .is_err());
        assert!(GuestRequests::open(
            crate::config::test_config().policy,
            "",
            "TelemetryService",
            "session-a",
            &path,
            Duration::from_secs(1),
        )
        .is_err());
    }
}
