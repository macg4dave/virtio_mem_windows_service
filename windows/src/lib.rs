pub mod config;
pub mod controller;
pub mod demand;
pub mod error;
pub mod event_log;
pub mod qga;
pub mod runtime;
pub mod service_host;
pub mod service_loop;
pub mod service_scm;
pub mod stats;
pub mod virtio_mem;
pub mod virtio_mem_provider;
pub mod virtio_mem_xml;

pub use config::ServiceConfig;
pub use controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
pub use demand::{
    DemandAgent, DemandAgentError, DemandCalculator, DemandError, DemandLimits, DemandPolicyConfig,
    DemandRecommendation, DemandReport, DemandReportPublisher, DemandState,
    JsonLinesDemandReportPublisher, JsonLinesRawTelemetryPublisher, MemoryTelemetry,
    MemoryTelemetrySnapshot, NativeMemoryTelemetry, RawTelemetryEnvelope, RawTelemetryPublisher,
    SystemTelemetryClock, TelemetryClock,
};
pub use error::{
    ConfigurationError, MemoryStatsError, PollError, RuntimeWiringError, ServiceHostError,
    ServiceLoopError, VirtioMemError,
};
pub use event_log::{
    ServiceEvent, ServiceEventId, ServiceEventLevel, ServiceEventSink, WindowsEventLog,
};
pub use qga::{NamedPipeGuestAgent, DEFAULT_QGA_OPERATION_TIMEOUT};
pub use runtime::{
    DemandServiceWorker, GuestAgent, MemoryPoller, NativeTelemetryWorker, QgaPollingWorker,
    RawTelemetryWorker, ServiceRuntime,
};
pub use service_host::StopSignal;
pub use service_host::{ServiceHost, ServiceState, ServiceWorker};
pub use service_loop::{poll_once, run_polling_loop, MemoryStateProvider, ResizeRequestSink};
pub use service_scm::{
    install_service, remove_service, run_as_service, start_service, stop_service, ScmHandler,
    ScmServiceState, ScmServiceStatus, WindowsServiceRegistration,
};
pub use stats::{parse_memory_stats, MemoryStats};
pub use virtio_mem::{VirtioMemState, MIN_BLOCK_SIZE_BYTES, MIN_HEADROOM_BYTES};
pub use virtio_mem_provider::{VirtioMemXmlSource, XmlMemoryStateProvider};
pub use virtio_mem_xml::{parse_virtio_mem_xml, VirtioMemXmlError, VirtioMemXmlState};
