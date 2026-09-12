//! Platform-neutral virtio-mem state, policy, and QEMU Guest Agent parsing.

pub mod behavior_evidence;
pub mod compatibility;
pub mod controller;
pub mod demand;
pub mod error;
pub mod global_pool;
pub mod reconciler;
pub mod shrink_recovery;
pub mod stats;
pub mod target_controller;
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
    AllocationProvenance, DemandCalculator, DemandError, DemandLimits, DemandPolicyConfig,
    DemandRecommendation, DemandReport, DemandState, MemoryResourceNotificationState,
    MemoryTelemetrySnapshot, OptionalTelemetrySignal, PagingActivitySnapshot,
    RawTelemetryContractMode, RawTelemetryEnvelope, ReusableMemorySnapshot, TelemetryCapability,
    TelemetrySignalStatus, TelemetrySource, TelemetryWarmup, WindowsNativeTelemetry,
    WindowsTelemetryCapabilities, DEMAND_REPORT_VERSION, LEGACY_RAW_TELEMETRY_VERSION,
    RAW_TELEMETRY_VERSION, WINDOWS_NATIVE_TELEMETRY_VERSION,
};
pub use error::{MemoryStatsError, PollError, ServiceLoopError, VirtioMemError};
pub use global_pool::{
    arbitrate_global_pool, GlobalPoolConfig, GlobalPoolDecision, GlobalPoolError, GlobalPoolPlan,
    HostPressureState, VmPoolInput,
};
pub use reconciler::{
    reconcile, ControlHealth, ReconcileAction, ReconcileDecision, ReconcileDirection,
    ReconcileError, ReconcileInput,
};
pub use shrink_recovery::{
    AbandonAction, AbandonToCurrent, ShrinkAction, ShrinkObservation, ShrinkOperation,
    ShrinkPolicy, ShrinkState,
};
pub use stats::{parse_memory_stats, parse_memory_stats_with_id, MemoryStats};
pub use target_controller::{
    calculate_instantaneous, CandidateHistoryEntry, InstantaneousTarget, TargetEstimate,
    TargetEstimator, TargetEstimatorError, TargetEstimatorState, TargetGeometry,
    TargetPolicyConfig, TargetSample, MAX_TARGET_HISTORY_ENTRIES, TARGET_ESTIMATOR_STATE_VERSION,
};
pub use units::{bytes_to_kibibytes, kibibytes_to_bytes, BYTES_PER_KIB};
pub use virtio_mem::{VirtioMemState, MIN_BLOCK_SIZE_BYTES, MIN_HEADROOM_BYTES};
pub use virtio_mem_xml::{
    parse_virtio_mem_xml, parse_virtio_mem_xml_for_alias, VirtioMemXmlError, VirtioMemXmlState,
};
