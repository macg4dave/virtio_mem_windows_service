//! Platform-neutral virtio-mem state, policy, and QEMU Guest Agent parsing.

pub mod controller;
pub mod error;
pub mod stats;
pub mod virtio_mem;
pub mod virtio_mem_xml;

pub use controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
pub use error::{MemoryStatsError, PollError, ServiceLoopError, VirtioMemError};
pub use stats::{parse_memory_stats, MemoryStats};
pub use virtio_mem::{VirtioMemState, MIN_BLOCK_SIZE_BYTES};
pub use virtio_mem_xml::{
    parse_virtio_mem_xml, parse_virtio_mem_xml_for_alias, VirtioMemXmlError, VirtioMemXmlState,
};
