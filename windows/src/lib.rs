pub mod config;
pub mod demand;
pub mod error;
pub mod event_log;
pub mod runtime;
pub mod service_host;
pub mod service_scm;

pub use config::ServiceConfig;
#[cfg(windows)]
pub use demand::{native_memory_resource_notifications, WindowsMemoryResourceNotificationApi};
pub use demand::{
    process_session_id, AtomicRawTelemetryPublisher, DemandError, JsonLinesRawTelemetryPublisher,
    MemoryResourceNotificationApi, MemoryResourceNotificationCollector,
    MemoryResourceNotificationKind, MemoryResourceNotificationState,
    MemoryResourceNotificationTelemetry, MemoryTelemetry, MemoryTelemetrySnapshot,
    NativeMemoryTelemetry, OptionalTelemetrySignal, PagingActivitySnapshot,
    RawTelemetryContractMode, RawTelemetryEnvelope, RawTelemetryPublisher, ReusableMemorySnapshot,
    SystemTelemetryClock, TelemetryCapability, TelemetryClock, TelemetrySignalStatus,
    TelemetryWarmup, UnavailableMemoryResourceNotifications, WindowsNativeTelemetry,
    WindowsTelemetryCapabilities,
};
pub use error::{ConfigurationError, RuntimeWiringError, ServiceHostError};
pub use event_log::{
    ServiceEvent, ServiceEventId, ServiceEventLevel, ServiceEventSink, WindowsEventLog,
};
pub use runtime::{NativeTelemetryWorker, RawTelemetryWorker};
pub use service_host::StopSignal;
pub use service_host::{ServiceHost, ServiceState, ServiceWorker};
pub use service_scm::{
    install_service, remove_service, run_as_service, start_service, stop_service, ScmHandler,
    ScmServiceState, ScmServiceStatus, WindowsServiceRegistration,
};
