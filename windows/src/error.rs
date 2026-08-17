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
