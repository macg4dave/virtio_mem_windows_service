# Data Model

## Memory State

### Controller State

- `current_free_bytes`: Last `stat-free` value from QEMU Guest Agent
- `current_available_bytes`: Last `stat-available` value, or `stat-free` when unavailable
- `current_total_bytes`: Total allocated memory in Windows
- `target_requested_bytes`: Next size to request from virtio-mem
- `virtio_mem_requested_bytes`: Live requested size from virtio-mem XML
- `virtio_mem_current_bytes`: Live active size from virtio-mem XML
- `min_memory_gb`: Minimum allocation (default 8 GB)
- `max_memory_gb`: Maximum allocation (default 28 GB)
- `lower_threshold_gb`: Free memory threshold to trigger add (default 2 GB)
- `upper_threshold_gb`: Free memory threshold to trigger remove (default 6 GB)

### Resize policy

The controller evaluates the last parsed `stat-free` value once per poll:

- Below `lower_threshold_bytes`: request one additional device block.
- Above `upper_threshold_bytes`: request one fewer device block.
- Between or exactly at either threshold: do not change memory.
- While `virtio_mem_requested_bytes != virtio_mem_current_bytes`: wait and do
  not issue another request.

Every target is clamped to the configured minimum and maximum and both limits
must be aligned to `block_size_bytes`.

### Persistence

Currently, state is transient (no database). State is recalculated on each poll cycle.

The controller treats `virtio_mem_current_bytes` as authoritative for calculating the next step. A resize is suppressed while requested and current sizes have not converged.

### Future State Storage

If persistent state is needed, track:

- State change history
- Poll cycle metrics
- Performance tuning parameters
