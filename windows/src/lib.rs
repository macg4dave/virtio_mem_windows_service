pub mod config;
pub mod controller;
pub mod error;
pub mod qga;
pub mod runtime;
pub mod service_host;
pub mod service_loop;
pub mod service_scm;
pub mod stats;

pub use config::ServiceConfig;
pub use controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
pub use error::{
    ConfigurationError, MemoryStatsError, PollError, ServiceHostError, ServiceLoopError,
};
pub use qga::NamedPipeGuestAgent;
pub use runtime::{GuestAgent, MemoryPoller, ServiceRuntime};
pub use service_host::StopSignal;
pub use service_host::{ServiceHost, ServiceState, ServiceWorker};
pub use service_loop::{poll_once, run_polling_loop, MemoryStateProvider, ResizeRequestSink};
pub use service_scm::{install_service, stop_service, ScmHandler, ScmServiceState, ScmServiceStatus, WindowsServiceRegistration};
pub use stats::{parse_memory_stats, MemoryStats};
