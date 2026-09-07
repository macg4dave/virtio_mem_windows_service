# Data Model

## Memory State

### Legacy policy input state

- `current_free_bytes`: Last free/unused value from the selected stats adapter
- `current_available_bytes`: Last available value, or free/unused fallback
- `current_total_bytes`: Adapter-specific total-like value; it is not
  authoritative virtio-mem allocation
- `target_requested_bytes`: Next size to request from virtio-mem
- `virtio_mem_requested_bytes`: Live requested size from virtio-mem XML
- `virtio_mem_current_bytes`: Live active size from virtio-mem XML
- `min_memory_bytes`: Minimum allocation, represented internally in bytes
- `max_memory_bytes`: Maximum allocation, represented internally in bytes
- `lower_threshold_bytes`: Free memory threshold to trigger add
- `upper_threshold_bytes`: Free memory threshold to trigger remove

### Phase 2 demand-agent state

The implemented native Windows collector produces a raw snapshot before policy
is applied:

- `physical_total_bytes`: `GlobalMemoryStatusEx` physical total
- `physical_available_bytes`: immediately reusable physical memory
- `memory_load_percent`: Windows memory-load estimate
- `commit_total_bytes`, `commit_limit_bytes`, `commit_peak_bytes`:
  `GetPerformanceInfo` commit values
- `system_cache_bytes`, `kernel_paged_bytes`, `kernel_nonpaged_bytes`:
  additional context from `GetPerformanceInfo`

Version 1 does **not** include a sample timestamp, VM identity, service/boot
session, sequence, or allocation provenance. Those are required additions for
M10d and must use a new schema version.

The derived demand state contains `physical_pressure`, `commit_pressure`, a
`demand_state` of `release`, `stable`, `want_more`, `pressure`, or `critical`,
`desired_target_bytes`, and `safe_floor_bytes`. The report is versioned as
`version: 1` and serializes all memory values as canonical byte counts.

The four policy levels have distinct meanings:

1. `configured_minimum_bytes`: administrative hard floor;
2. `safe_floor_bytes`: measured recommendation for pressure reclaim;
3. `desired_target_bytes`: normal operating recommendation;
4. observed current allocation: actual host/guest state.

Neither target authorizes a resize by itself. Phase 2 keeps allocation
authority in the existing host controller.

The `DemandAgent` runtime boundary accepts the observed current allocation as
an explicit input and publishes a complete report through an injected sink.
This keeps native telemetry and recommendation generation independent from QGA,
libvirt, and any future report transport. Publication failure is observable and
does not trigger a resize fallback.

No production owner currently supplies that allocation to the calculator.
M10c will move the production join to the host: Windows publishes raw telemetry
and the host combines it with alias-scoped live libvirt `current` before
calculating demand. Aggregate physical memory, configured limits, QGA totals,
and balloon `actual` are not allocation substitutes.

The M10d envelope must add VM and service identity, UTC sample time,
monotonic/session ordering, a boot or service-session identifier, sequence or
correlation identifier, and allocation-source/provenance metadata. Consumers
must reject stale, replayed, cross-VM, truncated, and unsupported-version
records according to documented bounds.

All memory quantities in the controller and host contract are unsigned 64-bit
byte counts. Human-readable GB/MiB values are presentation values only and
must be converted explicitly before entering the Rust policy layer. Internal
field names do not use implicit decimal or binary unit suffixes.

### Resize policy

The controller evaluates the last parsed `stat-free` value once per poll:

- Below `lower_threshold_bytes`: request one additional device block.
- Above `upper_threshold_bytes`: request one fewer device block.
- Between or exactly at either threshold: do not change memory.
- While `virtio_mem_requested_bytes != virtio_mem_current_bytes`: wait and do
  not issue another request.

Every target is clamped to the configured minimum and maximum and both limits
must be aligned to `block_size_bytes`.

Automatic shrink is not yet a supported Windows production path. M10b must add
a default-off shrink control and prove the roadmap's bounded same-target
re-notification and abandon-to-current recovery policy before the host may
enable automatic reclaim.

The planned host-only `ShrinkOperation` state contains an operation ID, VM and
device alias, block size, initial `requested`/`current`, immutable target,
latest current, blocks reclaimed and remaining, creation/progress times,
immutable deadline, retry index, and terminal reason. Its states are
`Observing`, `ShrinkStalled`, `RecoveryRequired`, `Recovering`, `Converged`, and
`Abandoned`. Progress is a block-aligned decrease of live `current`; it updates
the progress time but never the retry count or deadline. The live XML remains
authoritative—this record grants no right to replay after cancellation or
process restart. A restarted process treats unexplained divergence as
recovery-required observation.

The qualification profile uses five-second observation, no-progress delays of
30, 60, and 120 seconds, at most three exact-target re-notifications, and a
300-second deadline. Abandon-to-current is a distinct one-shot recovery with a
30-second deadline and no retry. Configuration will keep automatic shrink and
re-notification as independent default-off booleans and validate every timing,
retry, reclaim-quantum, and safe-floor bound before worker startup.

### Host memory-stat snapshot

The current `dommemstat` adapter maps libvirt balloon counters into the legacy
`MemoryStats` shape. Libvirt `actual` is the current balloon value; it does not
include virtio-mem memory and must not bound `unused` or `available`. An
`available > actual` sample is therefore not inherently inconsistent.

M9e introduces an explicit source snapshot with observation time and
`last-update` provenance. Policy must reject missing, stale, future, and
non-advancing samples according to configured bounds. The alias-scoped live
libvirt `current` field remains the only authoritative virtio-mem allocation
input. The QGA-shaped stats adapter is experimental and valid only for a
separately identified custom/downstream guest agent.

### Live XML semantics

The host-side state model is intentionally conservative because virtio-mem is not instantaneous. The libvirt live XML reports the following values as a snapshot of host-visible guest memory state:

- `requested`: the host's desired memory capacity for the virtio-mem device
- `current`: the memory currently exposed and usable by the guest
- `size`: the maximum memory the device can offer to the guest
- `block`: the hotplug granularity in bytes (for example 2 MiB = 2,097,152 bytes)

The live state is not a binary success flag. A request can be accepted by QEMU and still remain pending for some time while the guest kernel plugs or unplugs blocks. The controller therefore treats `current` as the authoritative safety boundary for the next decision, and it does not send another request until `requested` and `current` converge.

Compatibility evidence is refreshed separately from state. The host reads
`dynamic-memslots` and `unplugged-inaccessible` from the alias-selected live
QOM device through bounded QMP requests, merges that evidence with any XML
values, and requires an explicit operator workload review. Missing, disabled,
malformed, or conflicting evidence prevents resize preparation.

M9d extends that evidence with a fingerprint over the live domain/QEMU
configuration, memory backend and NUMA placement, memory-slot and VFIO mapping
budgets, incompatible device/workload classes, active balloon-resize state,
topology, and deployed versions.

This model is aligned with libvirt behavior: a resize request is serviced asynchronously, and the guest's ability to free memory or hotunplug blocks can delay or prevent full convergence.

The pure Rust `VirtioMemState` contract validates that device size, requested,
and current values are within `size` and block aligned. Observed `requested`
and `current` may both be zero when the device is fully unplugged; a proposed
resize target must remain positive, within `size`, block aligned, and subject
to the fixed device-headroom floor. Host XML parsing must construct and
validate this state before a resize sink can issue a request.

The upstream `viomem.sys` driver has corresponding fields named
`requested_size` and `plugged_size`, plus a block bitmap. The Virtio memory
device specification defines both as read-only device-configuration values,
and the reviewed driver reads them directly before selecting plug or unplug
work. For the pinned QEMU/libvirt/driver stack, libvirt `requested` represents
the requested device allocation and libvirt `current` represents the
guest-cooperative plugged allocation.

The Rust data model does not ingest the driver's debug print as a second copy
of state. `virtio_mem_current_bytes` from the alias-scoped live libvirt
snapshot is the authoritative allocation value. Driver trace records, when
explicitly and safely captured, are diagnostic evidence about notification,
branch selection, and failure behavior rather than an accounting input.

### Correlated behavior evidence

`BehaviorEvidenceDocument` is the versioned, operation-scoped M10a2 model used
to validate captured behavior before making cross-layer claims. Its identity
is repeated in every sample so records from another operation, VM, or device
cannot be silently combined. Sequence and monotonic time are strictly
increasing; wall-clock time cannot move backwards. All memory fields have an
explicit `bytes` unit.

The required layers are alias-scoped libvirt state, Windows `viomem` health,
and host-controller state. Host state supplies converged `before` and `after`
endpoints with stable device geometry; intermediate observation and recovery
states may be divergent. `DriverTrace` is optional and remains diagnostic.
The model validates supplied driver values against host geometry but does not
use them for allocation accounting.

### Persistence

Currently, state is transient (no database). State is recalculated on each poll cycle.

The controller treats `virtio_mem_current_bytes` as authoritative for calculating the next step. A resize is suppressed while requested and current sizes have not converged.

### Future State Storage

If persistent state is needed, track:

- State change history
- Poll cycle metrics
- Performance tuning parameters
