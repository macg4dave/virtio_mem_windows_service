use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryStatsError {
    #[error("invalid QEMU Guest Agent response: {0}")]
    InvalidJson(String),
    #[error("QEMU Guest Agent response is missing {0}")]
    MissingStat(&'static str),
    #[error("QEMU Guest Agent response has inconsistent memory values")]
    InconsistentValues,
    #[error("memory controller configuration is invalid: {0}")]
    InvalidConfiguration(&'static str),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PollError {
    #[error("QEMU Guest Agent request failed: {0}")]
    GuestAgent(String),
    #[error(transparent)]
    MemoryStats(#[from] MemoryStatsError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServiceLoopError {
    #[error("polling interval must be greater than zero")]
    InvalidInterval,
    #[error("memory state provider failed: {0}")]
    StateProvider(String),
    #[error(transparent)]
    Poll(#[from] PollError),
    #[error("resize request failed: {0}")]
    ResizeRequest(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ServiceHostError {
    #[error("service host is already running")]
    AlreadyRunning,
    #[error("service host has already stopped")]
    AlreadyStopped,
    #[error("service worker initialization failed: {0}")]
    Startup(String),
    #[error("service worker failed: {0}")]
    Worker(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigurationError {
    #[error("service configuration field is empty: {0}")]
    EmptyField(&'static str),
    #[error("polling interval must be greater than zero")]
    InvalidPollInterval,
    #[error("shutdown timeout must be greater than zero")]
    InvalidShutdownTimeout,
}
