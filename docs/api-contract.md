# API Contract

## QEMU Guest Agent Interface

### GetMemoryStats

The guest-side transport sends the request as one newline-delimited JSON
message over the configured Windows virtio-serial/QEMU Guest Agent pipe. The
pipe path is supplied to `NamedPipeGuestAgent`; the client does not discover
or modify host-side libvirt resources.

Request:

```json
{
  "execute": "guest-get-memory-stats",
  "id": "virtio-mem-memory-stats-1"
}
```

Response:

```json
{
  "return": [
    { "stat": "stat-free", "value": 2147483648 },
    { "stat": "stat-total", "value": 8589934592 },
    ...
  ]
}
```

### Expected Fields

- `stat-free`: Free memory in bytes
- `stat-total`: Total allocated memory in bytes
- `stat-available`: Available memory (including cache)

The Rust parser requires `stat-free` and `stat-total`. If `stat-available` is
omitted by the guest agent, it falls back to `stat-free`. Values greater than
`stat-total` are rejected as inconsistent. The Windows transport requires a
matching response `id`, a `return` array, and exactly one newline-delimited
response frame. The compatibility parser used by host-side adapters accepts
responses without an id because the host request remains a separate adapter
boundary.

### Host memory-stat source (`VIRTIO_MEM_STATS_SOURCE`)

The RHEL host controller's connected guest agent (QGA 109.1.0 on `win11_gpu`)
does not implement `guest-get-memory-stats` (see `docs/issues.md` ISSUE-001),
so the controller cannot rely on that command alone. `HostConfig` selects the
memory-stat source with `VIRTIO_MEM_STATS_SOURCE`:

- `dommemstat` (default): reads `virsh dommemstat <vm>`, a virtio-balloon
  driver counter that does not require the guest agent. It requires the
  domain to have `actual` and `unused` fields; `available` is used if present,
  otherwise `unused` is reused. This must be verified against the live guest
  before enabling automated resizing, since it depends on a functioning
  virtio-balloon driver/service in the guest.
- `qga`: uses `guest-get-memory-stats` as before, for guest agents that
  implement it.

Both sources produce the same `MemoryStats { free_bytes, available_bytes,
total_bytes }` value consumed by the shared policy engine, so switching
sources does not change `plan_resize` behavior.

## Phase 2 Windows demand report

The demand-agent contract is additive to the existing QGA/dommemstat contract.
It describes what Windows observes and recommends; it does not grant the
guest authority to allocate host memory.

The implemented versioned report is:

```json
{
  "version": 1,
  "memory": {
    "physical_total_bytes": 17179869184,
    "physical_available_bytes": 3221225472,
    "memory_load_percent": 72,
    "commit_total_bytes": 11811160064,
    "commit_limit_bytes": 25769803776,
    "commit_peak_bytes": 12884901888,
    "system_cache_bytes": 2147483648,
    "kernel_paged_bytes": 104857600,
    "kernel_nonpaged_bytes": 52428800
  },
  "demand": {
    "state": "pressure",
    "physical_pressure": 0.82,
    "commit_pressure": 0.46,
    "desired_target_bytes": 21474836480,
    "safe_floor_bytes": 17179869184
  },
  "limits": {
    "configured_minimum_bytes": 8589934592,
    "configured_maximum_bytes": 30064771072
  }
}
```

`GlobalMemoryStatusEx` is the implemented source for physical totals,
available physical memory, and memory load. `GetPerformanceInfo` is the
implemented source for commit and system-wide memory fields. The native
collector and report calculator are locally tested, but live workload evidence
and production service wiring remain open. The existing QGA/dommemstat report
remains valid during the transition.

Demand states are `release`, `stable`, `want_more`, `pressure`, and `critical`.
The current provisional pressure bands use the larger of physical and commit
pressure: below 0.25 is `release`, below 0.60 is `stable`, below 0.75 is
`want_more`, below 0.90 is `pressure`, and otherwise `critical`. These bands
are policy inputs, not live-VM evidence, and must be tuned only after measured
workload validation. A desired target is a recommendation, not a host
allocation grant.

`MemoryTelemetrySnapshot` validates counters before calculation. Physical and
commit values are canonical `u64` bytes; `GetPerformanceInfo` page counters
are converted with checked multiplication using the reported Windows page
size. The native collector returns an explicit error for failed Windows APIs,
zero denominators, impossible counters, and arithmetic overflow.

`DemandCalculator` clamps recommendations to configured byte limits and aligns
every target to the configured block size. It produces a one-block conservative
safe-floor recommendation, but neither that floor nor the desired target is a
resize command. The Windows service remains advisory and the existing host
controller remains the only Phase 2 actuation authority.

`DemandAgent` provides the runtime boundary for one caller-selected poll cycle:
it collects a snapshot, calculates a report using the observed current
allocation, and passes the report to an injected `DemandReportPublisher`. A
collection or publication failure is returned explicitly. The publisher has no
resize interface; integration with the main SCM worker and a persistent/event
report sink remain separate operational work.

`JsonLinesDemandReportPublisher` is the current durable local sink. It appends
one complete JSON object plus a newline to the configured report path and
returns directory, encoding, write, and flush failures explicitly. The generic
`DemandServiceWorker` uses this publication boundary when supplied with a
validated current-allocation provider; the main SCM worker does not guess that
state from QGA totals or configured limits.

## Memory Change Request

Input: Target memory size in bytes, aligned to the device block size.
Process: the host adapter converts the canonical byte target to an integer
KiB value, then runs `virsh update-memory-device <vm> --alias
<virtio-mem-alias> --requested-size <kib> --live`. `virsh` interprets
`--requested-size` as KiB by default; a target that is not an exact number of
KiB must be rejected rather than rounded.

The controller must inspect live virtio-mem XML after every request. `requested` is the desired size and `current` is the size currently active in the guest; they may differ while QEMU converges.

### Host-side virtio-mem XML contract

The official libvirt/QEMU model treats virtio-mem as a NUMA-aware memory balloon that is resized by changing the live `requested` value, not by hotplugging a new device. The live XML exposes four relevant values for each memory device:

- `size`: maximum memory the device can currently expose to the guest
- `block`: hotplug granularity; it must be a power of two and at least 1 MiB
  in the canonical byte contract
- `requested`: desired memory exposure for the guest
- `current`: actual memory currently in use by the guest

`requested` must be an integer multiple of `block` and must never exceed `size`. `current` may lag behind `requested` while the guest reclaims or plugs blocks; the controller must treat `requested != current` as an in-flight resize and avoid issuing another change until the guest settles.

When more than one virtio-mem device is present, `virsh` must be directed with `--alias` because the update API cannot infer which device should be resized. The host-side controller should therefore treat the alias as part of the contract and should validate the live XML against the selected alias after each request.

### Virtio-mem compatibility gate

before a resize sink may issue `update-memory-device`, the combined evidence
must explicitly confirm both `dynamic-memslots` and
`unplugged-inaccessible`. Missing or unrecognized attributes are represented as
`Unknown` and fail closed; they are never treated as enabled by default. XML
evidence may be merged with an independent QEMU/configuration evidence source,
but conflicting evidence is rejected. The combined gate also requires
separate operator evidence that the VM/workload does not use incompatible
classes such as `vfio-nvme`, RDMA migration, `mlock`, or unsupported vhost-user
workloads. Those workload facts cannot be inferred reliably from the
virtio-mem memory element alone.

This is a key operational difference from a DIMM or balloon model: virtio-mem is not a simple single-step memory resize, and guest cooperation is required to unplug or plug memory blocks safely.

### Driver and state terminology

The upstream Windows driver uses `requested_size` and `plugged_size`, while
libvirt exposes `requested` and `current`. The driver also maintains a
block-state bitmap and performs Windows memory-manager hot-add/hot-remove.
These observations support the host-side asynchronous model, but the mapping
between driver `plugged_size` and libvirt `current` is not yet a validated
cross-layer contract. Until Phase 3 validation completes, the existing live
libvirt `current` field remains authoritative for this repository's host
controller.

The pure `parse_virtio_mem_xml` adapter accepts a captured libvirt XML
snapshot, requires the `virtio-mem` model and alias, converts `B`, `KiB`,
`MiB`, and `GiB` values to canonical bytes with checked arithmetic, and
constructs a validated `VirtioMemState`. It performs no host command execution
or live XML discovery.

All external KiB boundaries use checked conversion helpers. Byte values sent to
`virsh --requested-size` must be exactly divisible by 1024; XML and
`dommemstat` KiB values reject multiplication overflow rather than saturating
or rounding. The shared state contract also requires the device size itself to
be an exact multiple of the block size, so every representable target is a
whole number of blocks.

`XmlMemoryStateProvider` adapts a caller-provided `VirtioMemXmlSource` to the
polling boundary. A source may obtain a snapshot from an approved host-side
integration, but the Windows service boundary itself remains limited to the
source trait and never invokes `virsh` or Linux commands.

## Stability Rules

- Do not change request/response format without updating this document
- Maintain backward compatibility with existing QEMU Guest Agent versions
- Version any breaking changes to the protocol
- Do not issue another resize while `requested` and `current` differ

## Controller Decision Contract

The Rust controller consumes parsed memory stats plus the live virtio-mem
`requested` and `current` values. It returns one of:

- `NoChange` when memory is within the hysteresis band or a safe limit has
  been reached
- `WaitForConvergence` when a previous resize is still pending
- `Request { requested_bytes }` for one aligned block of growth or removal

All `*_bytes` values are `u64` byte counts. No implicit conversion from GB,
MiB, pages, or blocks is permitted at this boundary. A host adapter must
validate the device `size`, `block`, `requested`, `current`, and proposed target
before forwarding a resize request.

The controller never emits a target outside the configured minimum/maximum
range and does not perform the host-side resize itself.

## RHEL host controller contract

Each systemd instance is configured with exactly one non-empty VM name and one
virtio-mem alias. The alias is restricted to letters, digits, `_`, `.`, and
`-`. The controller invokes `virsh` with a fixed argument vector; it does not
use a command shell. Its host calls are:

- `virsh qemu-agent-command <vm> {"execute":"guest-get-memory-stats"}`
- `virsh dumpxml <vm>` (the default for a running domain; `--inactive` is not
  used for live resize validation)
- `virsh update-memory-device <vm> --alias <alias> --requested-size <kib> --live`

The implementation must bound each command, capture a non-zero exit status
with its diagnostic output, and treat it as an explicit failure. Before the
update command, the controller must read and validate a fresh XML snapshot for
the configured alias and require `requested == current`. A successful command
response does not prove completion: subsequent snapshots decide convergence.
The controller never replays a resize request after a process restart.

## Guest Polling Boundary

`MemoryPoller` obtains a `GetMemoryStats` response through the `GuestAgent`
trait, parses it, and evaluates the controller decision. The service loop
obtains a full `VirtioMemState` snapshot through `MemoryStateProvider`; this
includes `size`, `block`, `requested`, and `current` in bytes. The loop
validates the snapshot and any proposed target before passing a `Request`
decision to `ResizeRequestSink`. `NoChange` and `WaitForConvergence` produce
no resize side effect.

## Polling Lifecycle

`run_polling_loop` executes one poll, waits for the configured non-zero
interval, and repeats until its `AtomicBool` stop signal is set. Polling and
resize errors stop the loop and are returned to the service host; failed
operations are not retried implicitly.

## Service Lifecycle

`ServiceHost` owns the worker stop signal and tracks `Created`, `Running`,
`Stopped`, and `Failed` states. Worker failures are returned to the caller and
transition the host to `Failed`; workers are not silently restarted. Windows
Service Control Manager registration and callbacks remain a platform adapter
around this lifecycle boundary.
