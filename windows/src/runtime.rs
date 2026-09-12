use std::time::Duration;

use crate::demand::{
    MemoryResourceNotificationTelemetry, MemoryTelemetry, MemoryTelemetrySnapshot,
    RawTelemetryEnvelope, RawTelemetryPublisher, TelemetryClock,
    UnavailableMemoryResourceNotifications,
};
use crate::service_host::{ServiceWorker, StopSignal};

/// Windows-native telemetry worker used by the service lifecycle.
#[derive(Debug)]
pub struct NativeTelemetryWorker<T> {
    telemetry: T,
    interval: Duration,
}

impl<T> NativeTelemetryWorker<T>
where
    T: MemoryTelemetry,
{
    pub fn new(telemetry: T, interval: Duration) -> Result<Self, String> {
        if interval.is_zero() {
            return Err("native telemetry polling interval must be greater than zero".to_owned());
        }
        Ok(Self {
            telemetry,
            interval,
        })
    }

    pub fn poll_once(&self) -> Result<MemoryTelemetrySnapshot, String> {
        self.telemetry
            .collect()
            .map_err(|error| format!("native memory telemetry failed: {error}"))
    }
}

impl<T> ServiceWorker for NativeTelemetryWorker<T>
where
    T: MemoryTelemetry + Send + 'static,
{
    fn initialize(&mut self, _stop: &StopSignal) -> Result<(), String> {
        self.poll_once().map(|_| ())
    }

    fn run(&mut self, stop: &StopSignal) -> Result<(), String> {
        while !stop.is_cancelled() {
            self.poll_once()?;
            if !stop.is_cancelled() {
                stop.wait(self.interval);
            }
        }
        Ok(())
    }
}

/// Production worker: publishes only raw, VM-scoped Windows telemetry.
/// It has no current-allocation input and no resize interface.
#[derive(Debug)]
pub struct RawTelemetryWorker<T, P, C, N = UnavailableMemoryResourceNotifications> {
    telemetry: T,
    memory_resource_notifications: N,
    publisher: P,
    clock: C,
    vm_name: String,
    service_name: String,
    session_id: String,
    next_sequence: u64,
    previous_monotonic_millis: Option<u64>,
    interval: Duration,
}

impl<T, P, C> RawTelemetryWorker<T, P, C, UnavailableMemoryResourceNotifications>
where
    T: MemoryTelemetry,
    P: RawTelemetryPublisher,
    C: TelemetryClock,
{
    pub fn new(
        telemetry: T,
        publisher: P,
        clock: C,
        vm_name: impl Into<String>,
        service_name: impl Into<String>,
        session_id: impl Into<String>,
        interval: Duration,
    ) -> Result<Self, String> {
        Self::with_memory_resource_notifications(
            telemetry,
            UnavailableMemoryResourceNotifications,
            publisher,
            clock,
            vm_name,
            service_name,
            session_id,
            interval,
        )
    }
}

impl<T, P, C, N> RawTelemetryWorker<T, P, C, N>
where
    T: MemoryTelemetry,
    P: RawTelemetryPublisher,
    C: TelemetryClock,
    N: MemoryResourceNotificationTelemetry,
{
    #[allow(clippy::too_many_arguments)]
    pub fn with_memory_resource_notifications(
        telemetry: T,
        memory_resource_notifications: N,
        publisher: P,
        clock: C,
        vm_name: impl Into<String>,
        service_name: impl Into<String>,
        session_id: impl Into<String>,
        interval: Duration,
    ) -> Result<Self, String> {
        let vm_name = vm_name.into();
        let service_name = service_name.into();
        let session_id = session_id.into();
        for (field, value) in [
            ("VM", vm_name.as_str()),
            ("service", service_name.as_str()),
            ("session", session_id.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 128 || !value.is_ascii() {
                return Err(format!(
                    "raw telemetry {field} identity must be non-empty ASCII of at most 128 bytes"
                ));
            }
        }
        if interval.is_zero() {
            return Err("raw telemetry polling interval must be greater than zero".to_owned());
        }
        Ok(Self {
            telemetry,
            memory_resource_notifications,
            publisher,
            clock,
            vm_name,
            service_name,
            session_id,
            next_sequence: 0,
            previous_monotonic_millis: None,
            interval,
        })
    }

    pub fn poll_once(&mut self) -> Result<RawTelemetryEnvelope, String> {
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
        let envelope = RawTelemetryEnvelope::new(
            self.vm_name.clone(),
            self.service_name.clone(),
            self.session_id.clone(),
            observed_unix_millis,
            monotonic_millis,
            self.next_sequence,
            memory,
        );
        let mut envelope = envelope;
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
        self.publisher
            .publish(&envelope)
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

impl<T, P, C, N> ServiceWorker for RawTelemetryWorker<T, P, C, N>
where
    T: MemoryTelemetry + Send + 'static,
    P: RawTelemetryPublisher + Send + 'static,
    C: TelemetryClock + Send + 'static,
    N: MemoryResourceNotificationTelemetry + Send + 'static,
{
    fn initialize(&mut self, _stop: &StopSignal) -> Result<(), String> {
        self.poll_once().map(|_| ())
    }

    fn run(&mut self, stop: &StopSignal) -> Result<(), String> {
        while !stop.is_cancelled() {
            stop.wait(self.interval);
            if !stop.is_cancelled() {
                self.poll_once()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demand::{
        DemandError, MemoryResourceNotificationState, OptionalTelemetrySignal,
        RawTelemetryContractMode, TelemetryCapability,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    const GIB: u64 = 1024 * 1024 * 1024;

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

    struct FailingTelemetry;

    impl MemoryTelemetry for FailingTelemetry {
        fn collect(&self) -> Result<MemoryTelemetrySnapshot, DemandError> {
            Err(DemandError::UnsupportedPlatform)
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

    #[derive(Debug)]
    struct FixedClock;

    impl TelemetryClock for FixedClock {
        fn now_unix_millis(&self) -> Result<u64, String> {
            Ok(1_000_000)
        }

        fn monotonic_millis(&self) -> Result<u64, String> {
            Ok(10)
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

    #[test]
    fn native_worker_validates_initial_telemetry() {
        let mut worker = NativeTelemetryWorker::new(TelemetryFixture, Duration::from_secs(1))
            .expect("worker should be constructible");
        worker
            .initialize(&StopSignal::new())
            .expect("native telemetry should validate");
    }

    #[test]
    fn native_worker_preserves_telemetry_failure() {
        let mut worker = NativeTelemetryWorker::new(FailingTelemetry, Duration::from_secs(1))
            .expect("worker should be constructible");
        assert_eq!(
            worker.initialize(&StopSignal::new()),
            Err("native memory telemetry failed: native Windows memory telemetry is unavailable on this platform".to_owned())
        );
    }

    #[test]
    fn raw_worker_publishes_without_allocation_input() {
        let mut worker = RawTelemetryWorker::new(
            TelemetryFixture,
            PublisherFixture::default(),
            FixedClock,
            "guest",
            "TelemetryService",
            "session-a",
            Duration::from_secs(1),
        )
        .expect("worker should be valid");

        let envelope = worker.poll_once().expect("raw publication should pass");
        assert_eq!(envelope.vm_name, "guest");
        assert_eq!(envelope.sequence, 0);
        assert_eq!(
            envelope.contract_mode(),
            Ok(RawTelemetryContractMode::WindowsNativeFallback),
            "WN1 publishes an explicit fallback until WN2 collects native pressure"
        );
        assert_eq!(worker.publisher().0, vec![envelope]);
        assert!(worker.poll_once().is_err(), "monotonic time must advance");
    }

    #[test]
    fn raw_worker_publishes_native_notification_state_without_policy_output() {
        let mut worker = RawTelemetryWorker::with_memory_resource_notifications(
            TelemetryFixture,
            FixedNotifications {
                state: MemoryResourceNotificationState::Low,
            },
            PublisherFixture::default(),
            FixedClock,
            "guest",
            "TelemetryService",
            "session-a",
            Duration::from_secs(1),
        )
        .expect("worker should be valid");

        let envelope = worker.poll_once().expect("raw publication should pass");
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
        let encoded = serde_json::to_string(&envelope).expect("encode envelope");
        assert!(!encoded.contains("desired"));
        assert!(!encoded.contains("requested"));
        assert!(!encoded.contains("current_bytes"));
    }

    #[cfg(windows)]
    #[test]
    fn raw_worker_publishes_a_real_native_notification_observation() {
        let mut worker = RawTelemetryWorker::with_memory_resource_notifications(
            TelemetryFixture,
            crate::demand::native_memory_resource_notifications(),
            PublisherFixture::default(),
            FixedClock,
            "guest",
            "TelemetryService",
            "session-a",
            Duration::from_secs(1),
        )
        .expect("worker should be valid");

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
        let dropped = Arc::new(AtomicBool::new(false));
        let mut worker = RawTelemetryWorker::with_memory_resource_notifications(
            TelemetryFixture,
            DropTrackedNotifications(dropped.clone()),
            PublisherFixture::default(),
            FixedClock,
            "guest",
            "TelemetryService",
            "session-a",
            Duration::from_secs(1),
        )
        .expect("worker should be valid");
        let stop = StopSignal::new();
        stop.cancel();

        assert_eq!(worker.run(&stop), Ok(()));
        assert!(worker.publisher().0.is_empty());
        drop(worker);
        assert!(dropped.load(Ordering::Acquire));
    }

    #[test]
    fn rejects_zero_interval() {
        assert!(RawTelemetryWorker::new(
            TelemetryFixture,
            PublisherFixture::default(),
            FixedClock,
            "guest",
            "TelemetryService",
            "session-a",
            Duration::ZERO,
        )
        .is_err());
    }
}
