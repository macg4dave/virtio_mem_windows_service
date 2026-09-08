//! Platform-neutral virtio-mem state, policy, and QEMU Guest Agent parsing.

pub mod behavior_evidence;
pub mod compatibility;
pub mod controller;
pub mod demand;
pub mod error;
pub mod stats;
pub mod units;
pub mod virtio_mem;
pub mod virtio_mem_xml;

pub use behavior_evidence::{
    parse_behavior_evidence, BehaviorEvidenceDocument, BehaviorEvidenceError, EvidenceIdentity,
    EvidenceKind, EvidenceSample, EvidenceUnit, HostEvidencePhase, BEHAVIOR_EVIDENCE_VERSION,
};
pub use compatibility::{
    CompatibilityEvidence, VirtioMemCompatibility, VirtioMemCompatibilityError,
};
pub use controller::{plan_resize, MemoryControllerConfig, ResizeDecision};
pub use demand::{
    DemandCalculator, DemandError, DemandLimits, DemandPolicyConfig, DemandRecommendation,
    DemandReport, DemandState, MemoryTelemetrySnapshot, RawTelemetryEnvelope,
    DEMAND_REPORT_VERSION, RAW_TELEMETRY_VERSION,
};
pub use error::{MemoryStatsError, PollError, ServiceLoopError, VirtioMemError};
pub use stats::{parse_memory_stats, parse_memory_stats_with_id, MemoryStats};
pub use units::{bytes_to_kibibytes, kibibytes_to_bytes, BYTES_PER_KIB};
pub use virtio_mem::{VirtioMemState, MIN_BLOCK_SIZE_BYTES, MIN_HEADROOM_BYTES};
pub use virtio_mem_xml::{
    parse_virtio_mem_xml, parse_virtio_mem_xml_for_alias, VirtioMemXmlError, VirtioMemXmlState,
};
