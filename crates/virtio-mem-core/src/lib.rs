//! Platform-neutral virtio-mem state, policy, and QEMU Guest Agent parsing.

pub mod compatibility;
pub mod controller_status;
pub mod demand;
pub mod error;
pub mod pool;
pub mod reconciler;
pub mod shrink_recovery;
pub mod target_controller;
pub mod units;
pub mod virtio_mem;
pub mod virtio_mem_xml;
pub mod windows_service_config;

pub use compatibility::{
    CompatibilityEvidence, VirtioMemCompatibility, VirtioMemCompatibilityError,
};
pub use controller_status::{
    parse_controller_status, AcceptedTelemetryIdentity, CapacityState, CommandOwnership,
    ControllerCommandStatus, ControllerStatusError, ControllerStatusSnapshot, ReclaimReadiness,
    RecoveryState, CONTROLLER_STATUS_VERSION, MAX_CONTROLLER_STATUS_BYTES,
};
pub use demand::{
    AllocationProvenance, DemandError, MemoryResourceNotificationState, MemoryTelemetrySnapshot,
    OptionalTelemetrySignal, PagingActivitySnapshot, RawTelemetryContractMode,
    RawTelemetryEnvelope, ReusableMemorySnapshot, TelemetryCapability, TelemetrySignalStatus,
    TelemetrySource, TelemetryWarmup, WindowsNativeTelemetry, WindowsTelemetryCapabilities,
    LEGACY_RAW_TELEMETRY_VERSION, RAW_TELEMETRY_VERSION, WINDOWS_NATIVE_TELEMETRY_VERSION,
};
pub use error::VirtioMemError;
pub use pool::{allocate_pool, PoolAllocation, PoolError, PoolMember};
pub use reconciler::{
    reconcile, ControlHealth, ReconcileAction, ReconcileDecision, ReconcileDirection,
    ReconcileError, ReconcileInput,
};
pub use shrink_recovery::{AbandonAction, AbandonToCurrent, ShrinkObservation};
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
pub use windows_service_config::{WindowsServiceConfigFile, WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION};
