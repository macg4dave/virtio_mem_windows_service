# Data Model

## Memory State

### Controller State

- `current_free_bytes`: Last `stat-free` value from QEMU Guest Agent
- `current_available_bytes`: Last `stat-available` value, or `stat-free` when unavailable
- `current_total_bytes`: Total allocated memory in Windows
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
- `sample_timestamp`: monotonic or UTC timestamp selected by the runtime

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

### Live XML semantics

The host-side state model is intentionally conservative because virtio-mem is not instantaneous. The libvirt live XML reports the following values as a snapshot of host-visible guest memory state:

- `requested`: the host's desired memory capacity for the virtio-mem device
- `current`: the memory currently exposed and usable by the guest
- `size`: the maximum memory the device can offer to the guest
- `block`: the hotplug granularity in bytes (for example 2 MiB = 2,097,152 bytes)

The live state is not a binary success flag. A request can be accepted by QEMU and still remain pending for some time while the guest kernel plugs or unplugs blocks. The controller therefore treats `current` as the authoritative safety boundary for the next decision, and it does not send another request until `requested` and `current` converge.

This model is aligned with libvirt behavior: a resize request is serviced asynchronously, and the guest's ability to free memory or hotunplug blocks can delay or prevent full convergence.

The pure Rust `VirtioMemState` contract validates that device size, requested,
current, and target values are positive, within `size`, and aligned to a
power-of-two block that is at least 1 MiB. Host XML parsing must construct and
validate this state before a resize sink can issue a request.

The upstream `viomem.sys` driver has corresponding fields named
`requested_size` and `plugged_size`, plus a block bitmap. Those names must not
be silently substituted for libvirt `requested` and `current`: their exact
cross-layer mapping remains a Phase 3 validation task. Until then,
`virtio_mem_current_bytes` from the live libvirt snapshot is the host
controller's authoritative active state.

### Persistence

Currently, state is transient (no database). State is recalculated on each poll cycle.

The controller treats `virtio_mem_current_bytes` as authoritative for calculating the next step. A resize is suppressed while requested and current sizes have not converged.

### Future State Storage

If persistent state is needed, track:

- State change history
- Poll cycle metrics
- Performance tuning parameters
