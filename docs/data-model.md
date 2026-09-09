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

The M10d `RawTelemetryEnvelope` version 2 contains the configured `vm_name` and
`service_name`, a process `session_id`, Unix observation milliseconds,
process-monotonic milliseconds, a session-local sequence, raw memory snapshot,
and explicit `windows_native_memory_apis` telemetry plus
`host_live_libvirt_current_required` allocation provenance. It intentionally
contains no allocation or recommendation. A new session begins at sequence
zero; records within a session must strictly increase sequence and monotonic
time.

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

Production calculation now occurs on the host. Windows publishes only the raw
envelope, and the host combines it with alias-scoped live libvirt `current`
before calculating demand. Aggregate physical memory, configured limits, QGA
totals, and balloon `actual` are not allocation substitutes. The host rejects
stale, future, wrong-VM, malformed, unsupported-version, invalid-counter, and
live-device-conflicting inputs before policy.

The host currently retains the accepted record and up to 16 retired session
identifiers in memory. It rejects replayed/non-monotonic records, retired
session reuse, files above 1 MiB, records above 64 KiB, and a final line without
a newline. Durable restart-safe acknowledgement, rotation/retention, atomic
reader handoff, and deployment ACLs remain M10d work.

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

Host service actuation uses directional quanta independent of the device's
minimum block size: one 1 GiB quantum when any pressure state requests growth,
and one 64 MiB quantum when release is advised. Each quantum must itself be a
multiple of the live device block size. Pressure severity affects arbitration,
not the size of a single per-VM request.

Automatic shrink is a default-on product capability; same-target diagnostic
re-notification remains independently default-off. Enabled reclaim still
requires fresh continuous telemetry, warmed history, a valid safe floor,
compatible converged state, current attestation, and an unlatched reconciler.
A live 64 MiB request made no progress, so that run is negative platform-
qualification evidence rather than a reason to make the product growth-only.
M10b's bounded retry and abandon-to-current models remain diagnostic/recovery
state; they do not calculate durable demand.

M10e-M10f add three separate controller values: `desired_bytes` is the
absolute policy target calculated from fresh guest demand, `requested_bytes`
is device intent, and `current_bytes` is authoritative actual allocation.
They are not controller states. Control health separately records converged,
growing, shrinking, constrained, command-unknown, recovery-required, and
latched conditions. A pending shrink may be superseded only upward on fresh
pressure. Partial or stalled reclaim leaves desired unchanged and is exposed
as constrained current state; a no-progress deadline is health evidence, not
a memory-sizing input. See the normative formulas and state table in
[`target-controller.md`](target-controller.md).

The host-only `ShrinkOperation` contains block size, immutable target, latest
current, creation/progress times, immutable deadline, retry index, and terminal
state. Its states are `Observing`, `Stalled`, `RecoveryRequired`, `Converged`,
and `Cancelled`. Progress is a block-aligned decrease of live `current`; it updates
the progress time but never the retry count or deadline. The live XML remains
authoritative—this record grants no right to replay after cancellation or
process restart. A restarted process treats unexplained divergence as
recovery-required observation.

The qualification profile uses five-second observation, no-progress delays of
30, 60, and 120 seconds, at most three exact-target re-notifications, and a
300-second deadline. Abandon-to-current is a distinct one-shot recovery with a
30-second deadline and no retry. Configuration keeps automatic shrink and
re-notification independent: shrink defaults on and re-notification defaults
off. Target construction continues to enforce bounded reclaim and safe-floor
rules.

### Host memory-stat snapshot

The `dommemstat` adapter maps libvirt balloon counters into the legacy
`MemoryStats` shape without treating balloon `actual` as total memory.
`DomMemStatSnapshot` retains `balloon_actual_bytes` as provenance, maps
`unused` to `free_bytes`, and maps required `available` to `available_bytes`
and the total-like legacy bound. It also records observation time and
`last-update`. Missing, stale, future, non-advancing, malformed, duplicate, or
overflowing samples fail according to configured positive age/skew bounds.
The alias-scoped live libvirt `current` field remains the only authoritative
virtio-mem allocation input. The QGA-shaped stats adapter is experimental and
valid only for a separately identified custom/downstream guest agent.

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

The M9d compatibility-attestation document has `version = 1`, `evidence`,
`review`, and `fingerprint_sha256`. Evidence identifies the VM and alias and
stores SHA-256 values for allocation-neutral live domain XML, allocation-neutral
native QEMU arguments, and libvirt version output, plus the parsed QMP QEMU
version and both required QOM booleans. Review stores positive confirmations
for the supported trusted-development workload exclusions, positive memory-slot
and VFIO mapping budgets, and a non-empty Windows driver version. The document
fingerprint is SHA-256 over canonical JSON containing `version`, `evidence`,
and `review`; it is verified before live evidence is compared. Allocation
progress alone does not change the fingerprint, but backend/page/NUMA, device,
slot/VFIO, balloon, topology, identity, property, trust, workload, driver, QEMU,
or libvirt changes revoke authorization.
Review and complete-attestation documents are UTF-8 JSON limited to 65,536
bytes and reject unknown fields.

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

M10d replay acknowledgement is persistent. The legacy directional policy and
M10b operation ownership remain process-local. M10e-M10f add a versioned,
bounded, atomically replaced host checkpoint for qualified target history,
durable desired, fingerprints, command intent, and the actuation latch. This is
a small state file rather than a database.

The controller treats `virtio_mem_current_bytes` as authoritative. Ordinary
resize is suppressed while requested and current differ; M10f adds only the
validated upward-supersession exception defined in the target-controller
contract.

### Future State Storage

If persistent state is needed, track:

- State change history
- Poll cycle metrics
- Performance tuning parameters
