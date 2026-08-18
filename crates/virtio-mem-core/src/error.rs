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
pub enum VirtioMemError {
    #[error("virtio-mem device size must be greater than zero")]
    ZeroSize,
    #[error("virtio-mem block size {actual} is smaller than the minimum {minimum} bytes")]
    BlockSizeTooSmall { actual: u64, minimum: u64 },
    #[error("virtio-mem block size must be a power of two: {0} bytes")]
    BlockSizeNotPowerOfTwo(u64),
    #[error("virtio-mem {name} value {value} is outside device size {size}")]
    ValueOutsideSize {
        name: &'static str,
        value: u64,
        size: u64,
    },
    #[error("virtio-mem {name} value {value} is not aligned to block size {block}")]
    ValueNotAligned {
        name: &'static str,
        value: u64,
        block: u64,
    },
}
