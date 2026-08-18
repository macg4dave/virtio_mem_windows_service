//! Platform-neutral virtio-mem state, policy, and QEMU Guest Agent parsing.

pub mod compatibility;
pub mod controller;
pub mod error;
pub mod stats;
pub mod units;
pub mod virtio_mem;
pub mod virtio_mem_xml;

pub use compatibility::{
    CompatibilityEvidence, VirtioMemCompatibility, VirtioMemCompatibilityError,
};
pub use controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
pub use error::{MemoryStatsError, PollError, ServiceLoopError, VirtioMemError};
pub use stats::{parse_memory_stats, parse_memory_stats_with_id, MemoryStats};
pub use units::{bytes_to_kibibytes, kibibytes_to_bytes, BYTES_PER_KIB};
pub use virtio_mem::{VirtioMemState, MIN_BLOCK_SIZE_BYTES, MIN_HEADROOM_BYTES};
pub use virtio_mem_xml::{
    parse_virtio_mem_xml, parse_virtio_mem_xml_for_alias, VirtioMemXmlError, VirtioMemXmlState,
};
