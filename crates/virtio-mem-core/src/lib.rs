//! Platform-neutral virtio-mem state, policy, and QEMU Guest Agent parsing.

pub mod behavior_evidence;
pub mod compatibility;
pub mod controller;
pub mod controller_status;
pub mod demand;
pub mod error;
pub mod global_pool;
pub mod pool_ledger;
pub mod pressure_assessment;
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
pub use controller_status::{
    parse_controller_status, AcceptedTelemetryIdentity, CapacityState, CommandOwnership,
    ControllerCommandStatus, ControllerStatusError, ControllerStatusSnapshot, PressureShadowStatus,
    ReclaimReadiness, RecoveryState, CONTROLLER_STATUS_VERSION, MAX_CONTROLLER_STATUS_BYTES,
};
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
    arbitrate_host_pool, device_target_from_total_ram, total_ram_from_device,
    validate_host_pool_policy, DemandProviderIdentity, DemandReportAvailability,
    DemandReportContinuity, GuestDemandReport, GuestPressureState, GuestShrinkEligibility,
    HostPoolError, HostPoolMemberPolicy, HostPoolPlan, HostPoolPolicy, PoolGrant,
    PoolGrantDisposition, PoolMemberIdentity, PoolMemberLifecycle, PoolMemberSnapshot,
    PoolSnapshotBlocker, PoolSnapshotBlockerReason, ReclaimForTransfer,
    GUEST_DEMAND_REPORT_VERSION, HOST_POOL_ARBITRATION_VERSION, HOST_POOL_PLAN_VERSION,
    HOST_POOL_POLICY_VERSION, MAX_DEMAND_REASON_CODES, MAX_POOL_MEMBERS,
};
pub use pool_ledger::{
    host_pool_policy_fingerprint, CommandRecovery, FilePoolLedgerStore, PoolCommandDirection,
    PoolCommandOwner, PoolCommandPhase, PoolLedger, PoolLedgerAccounting, PoolLedgerCommand,
    PoolLedgerError, PoolLedgerMember, PoolLedgerObservation, HOST_POOL_LEDGER_VERSION,
    MAX_HOST_POOL_LEDGER_BYTES,
};
pub use pressure_assessment::{
    assess_windows_pressure, select_pressure_growth, AssessmentAllocation, AssessmentAvailability,
    AssessmentConfidence, AssessmentInputIdentity, AssessmentReason, MemoryRequirementAssessment,
    PressureAssessmentError, PressureGrowthDecision, PressureGrowthMode, PressureHistoryEntry,
    PressureHistoryState, PressureHistorySummary, PressurePolicyConfig, PressurePolicyMode,
    PressureShadowComparison, PressureState, PressureStateAssessment, ShrinkSafetyAssessment,
    ShrinkSafetyState, WindowsPressureAssessment, MAX_PRESSURE_HISTORY_ENTRIES,
    PRESSURE_ASSESSMENT_VERSION, PRESSURE_HISTORY_VERSION, PRESSURE_POLICY_VERSION,
    PRESSURE_SHADOW_COMPARISON_VERSION,
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
    calculate_effective_maximum, calculate_instantaneous, CandidateHistoryEntry,
    InstantaneousTarget, TargetEstimate, TargetEstimator, TargetEstimatorError,
    TargetEstimatorState, TargetGeometry, TargetPolicyConfig, TargetSample,
    MAX_TARGET_HISTORY_ENTRIES, TARGET_ESTIMATOR_STATE_VERSION,
};
pub use units::{bytes_to_kibibytes, kibibytes_to_bytes, BYTES_PER_KIB};
pub use virtio_mem::{VirtioMemState, MIN_BLOCK_SIZE_BYTES, MIN_HEADROOM_BYTES};
pub use virtio_mem_xml::{
    parse_virtio_mem_xml, parse_virtio_mem_xml_for_alias, VirtioMemXmlError, VirtioMemXmlState,
};
