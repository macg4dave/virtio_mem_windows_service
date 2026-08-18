use thiserror::Error;

pub use virtio_mem_core::{MemoryStatsError, PollError, ServiceLoopError, VirtioMemError};

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
    #[error("configuration file I/O failed: {0}")]
    FileIo(String),
    #[error("configuration file is invalid: {0}")]
    InvalidFile(String),
    #[error("unsupported configuration schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("configuration duration cannot be represented in milliseconds")]
    DurationOverflow,
}
