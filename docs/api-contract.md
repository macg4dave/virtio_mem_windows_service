# API Contract

## Experimental custom guest-agent memory interface

### GetMemoryStats

`guest-get-memory-stats` is absent from the upstream QEMU Guest Agent schemas
reviewed for QEMU 9.1, 10.1, and master. The interface below documents the
repository's implemented parser/transport boundary; it is not an upstream QGA
contract. It may be enabled only for a separately identified and validated
custom/downstream guest agent that implements this exact extension.

The guest-side transport sends one newline-delimited JSON message over its
configured test/adapter pipe. The pipe path is supplied to
`NamedPipeGuestAgent`; the client does not discover or modify host-side
libvirt resources. Production Windows workers use native telemetry and do not
open the QGA-owned virtio-serial channel.

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
responses without an id because the host request remains a separate
experimental adapter boundary.

### Host memory-stat source (`VIRTIO_MEM_STATS_SOURCE`)

The RHEL host controller's connected guest agent (QGA 110.0.2 on `win11_gpu`)
does not implement the custom command. `HostConfig` selects the memory-stat
source with `VIRTIO_MEM_STATS_SOURCE`:

- `dommemstat` (default): reads `virsh dommemstat <vm>`, a virtio-balloon
  driver counter that does not require QGA. It requires `actual`, `unused`,
  `available`, and `last-update`. `actual` is retained only as balloon
  provenance; `unused` maps to free bytes and `available` supplies the
  available/total-like legacy bound. Both counters may exceed `actual`, but
  `unused > available` is inconsistent. The source rejects malformed,
  duplicate, overflowing, missing, stale, future, and non-advancing evidence
  using `VIRTIO_MEM_STATS_MAX_AGE_SECONDS` and
  `VIRTIO_MEM_STATS_FUTURE_TOLERANCE_SECONDS`.
- `qga`: uses the experimental `guest-get-memory-stats` extension. It must not
  be selected merely because an upstream QGA version is new enough; the exact
  advertised downstream capability has to be validated first.

Both current adapters produce the legacy `MemoryStats { free_bytes,
available_bytes, total_bytes }` value consumed by the shared policy engine.
That common shape does not make their `total_bytes` fields equivalent to live
virtio-mem allocation. Alias-scoped libvirt `current` remains authoritative.

## Phase 2 Windows demand report

The demand-agent contract is separate from host QGA health and `dommemstat`.
Version 1 describes what Windows observes and a locally calculated
recommendation; it does not grant the guest authority to allocate host memory.

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

Version 1 is a local foundation, not an ingestion-ready envelope. It has no
sample timestamp, VM/service identity, boot or service-session identifier,
sequence/correlation identifier, or current-allocation provenance. Consumers
must not infer freshness, reject replay, or join it to a VM allocation using
undocumented context. M10d will introduce a new schema version for those
fields rather than silently changing version 1.

`GlobalMemoryStatusEx` is the implemented source for physical totals,
available physical memory, and memory load. `GetPerformanceInfo` is the
implemented source for commit and system-wide memory fields. The native
collector and report calculator are locally tested, but live workload evidence
and production service wiring remain open. Host-side `dommemstat` remains the
verified Phase 2 policy source during the transition.

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

`JsonLinesDemandReportPublisher` is the current local append-only sink. It appends
one complete JSON object plus a newline to the configured report path and
returns directory, encoding, write, and flush failures explicitly. The generic
`DemandServiceWorker` uses this publication boundary when supplied with a
validated current-allocation provider; the main SCM worker does not guess that
state from QGA totals or configured limits. Version 1 has no rotation,
retention, maximum-file, reader acknowledgement, or atomic handoff contract
and must not be enabled as an unattended production spool until M10d defines
and tests those behaviors.

### Current-allocation ownership

Fresh live libvirt state is authoritative for Phase 2 allocation. The Windows
service has no supported driver status API and must not invoke libvirt or infer
allocation from aggregate physical memory. The M10c decision is a host-side
join: Windows publishes a fresh, versioned raw telemetry envelope; the host
joins it with alias-scoped live libvirt `current` and calculates the target.
There is no host-to-guest allocation feed and no guest resize authority. Until
M10c/M10d implement that contract, the production SCM worker validates native
telemetry but does not publish `DemandReport` values.

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

Observed `requested` and `current` may both be zero when the virtio-mem device
is fully unplugged. Zero is therefore valid live state, but it is not a valid
resize target: every requested target must be positive, block aligned, within
the device size, and satisfy the fixed device-headroom requirement.

When more than one virtio-mem device is present, `virsh` must be directed with
`--alias` because the update API cannot infer which device should be resized.
The host-side controller therefore treats the alias as part of the contract
and validates live XML against it after each request. Despite upstream
multi-device support, Phase 2 permits only one active controller/device on the
development host because independent instances do not share an atomic host
reservation. M11 owns expansion to multiple active targets.

### Virtio-mem compatibility gate

Before a host resize sink may issue `update-memory-device`, the combined evidence
must explicitly confirm both `dynamic-memslots` and
`unplugged-inaccessible`. Missing or unrecognized attributes are represented as
`Unknown` and fail closed; they are never treated as enabled by default. XML
evidence may be merged with independent QMP evidence, and conflicts are
rejected. The version-1 M9d attestation binds positive operator declarations
for vDPA, RDMA, VFIO-NVMe, `mlock`, secure virtualization, active balloon
resize, unsupported vhost-user, trust, slot/VFIO budgets, and driver version to
fresh live configuration evidence. Allocation-neutral hashes of full domain
XML and native QEMU argv cover backend page/sparse/reserve/preallocation/
sharing/core-dump properties, NUMA placement, devices, machine, and topology;
QMP and libvirt evidence bind deployed stack versions and required properties.

This is a key operational difference from a DIMM or balloon model: virtio-mem is not a simple single-step memory resize, and guest cooperation is required to unplug or plug memory blocks safely.

QEMU does not yet provide balloon-like protection against access to all
unplugged memory. A hard QEMU/libvirt cgroup memory limit is recommended
defense-in-depth for the fully trusted development/test `win11_gpu` guest and
is mandatory for untrusted or production guests.

The installed Windows driver has not been shown to retry an incomplete shrink
without another event. Automatic shrink must remain disabled by default until
M10b proves the selected bounded same-target re-notification and controlled
failed-shrink recovery path. The qualification profile observes every five
seconds, permits at most three exact-target re-notifications after 30, 60, and
120 seconds without block progress, and retains one immutable 300-second
operation deadline. Automatic shrink and re-notification are separate
default-off controls; neither is implemented yet.

The re-notification path is deliberately narrower than the ordinary resize
contract. It may run only when a controller-owned shrink has
`requested == immutable_target < current`; it must repeat the identical target
and revalidate fresh alias, compatibility, health, units, and ownership first.
Observed progress never resets the retry count or operation deadline. A stall
is a latched observable controller state, not a fatal error that invites a
service-manager restart. Cancellation, restart, external requested-size
change, invalid state, stale evidence, or ambiguous command outcome prohibits
replay and enters recovery-required observation.

M10b must separately qualify one non-disruptive recovery command. After two
stable fresh samples and an immediate pre-apply read, abandon-to-current raises
`requested` to the aligned observed `current`, preserving partial reclaim. It
has one 30-second convergence window and no retry. It remains operator-approved
only until live zero-progress and partial-progress cases pass; graceful domain
recreation from a known persistent definition remains the final
operator-approved fallback.

### Driver and state terminology

The upstream Windows driver uses `requested_size` and `plugged_size`, while
libvirt exposes `requested` and `current`. The driver also maintains a
block-state bitmap and performs Windows memory-manager hot-add/hot-remove.
The Virtio 1.2 memory-device contract defines the driver fields as read-only
device configuration: `requested_size` is the device's requested amount and
`plugged_size` is the amount represented by plugged blocks. The reviewed
virtio-win worker reads those fields directly and chooses plug or unplug work
by comparing them. QEMU/libvirt expose the same device intent and completed
allocation as `requested` and `current`.

This establishes the semantic mapping for the pinned stack without making the
Windows debug message a second allocation authority. Alias-scoped live
libvirt `requested` and `current` remain the repository contract; `current`
is authoritative for host allocation and another resize is forbidden until
the pair converges. Kernel-debug capture is optional diagnostic and
installed-driver behavior evidence. It is still required when a test claims
to explain a Windows notification, plug/unplug attempt, or no-progress shrink,
but it is not required to calculate host pool accounting.

### Correlated behavior-evidence format

M10a2 defines behavior-evidence document version `1` as a hermetic JSON
contract. A document carries one top-level identity (`operation_id`,
`vm_name`, and `device_alias`) and an ordered `samples` array. Every sample
must repeat that exact identity and provide a strictly increasing `sequence`
and `monotonic_millis`, a nondecreasing `wall_clock_unix_millis`, a nonempty
`source_id`, the explicit unit `bytes`, and one tagged evidence layer.

The required layers are `host_libvirt`, `windows_health`, and
`controller_state`; `driver_trace` is optional. Host samples carry stable
device geometry and phases named `before`, `observation`, `recovery`, or
`after`. Exactly one `before` and one later `after` endpoint are required and
both must be converged. Every host state is validated by `VirtioMemState`.

When driver trace is present, its `requested_size_bytes` and
`plugged_size_bytes` must fit and align to the host device geometry, but they
never replace live libvirt `current` as allocation authority. Parsing fails
closed on unsupported versions, malformed JSON, missing or unknown units,
invalid or mixed identity, non-monotonic ordering, missing required layers,
divergent endpoints, geometry drift, and invalid trace values. Documents are
limited to 10,000 samples and identifiers to 128 bytes.

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
- Never infer support for a custom QGA command from a QGA version number
- Version any breaking changes to the protocol
- Do not issue another resize while `requested` and `current` differ

## Controller Decision Contract

The Rust controller consumes parsed memory stats plus the live virtio-mem
`requested` and `current` values. It returns one of:

- `NoChange` when memory is within the hysteresis band or a safe limit has
  been reached
- `WaitForConvergence` when a previous resize is still pending
- `Request { requested_bytes }` for one aligned block of normal growth or
  removal. A converged device below the configured minimum instead receives
  one aligned bootstrap request to that minimum; this permits a fully
  unplugged device to enter the managed range without hundreds of intermediate
  requests.

All `*_bytes` values are `u64` byte counts. No implicit conversion from GB,
MiB, pages, or blocks is permitted at this boundary. A host adapter must
validate the device `size`, `block`, `requested`, `current`, and proposed target
before forwarding a resize request.

The controller never emits a target outside the configured minimum/maximum
range and does not perform the host-side resize itself. A converged allocation
above the configured maximum remains an inconsistent state and fails closed;
only the below-minimum bootstrap has explicit reconciliation behavior.

## RHEL host controller contract

Each systemd instance is configured with exactly one non-empty VM name and one
virtio-mem alias. The alias is restricted to letters, digits, `_`, `.`, and
`-`. The controller invokes `virsh` with a fixed argument vector; it does not
use a command shell. Its host calls are:

- optional `virsh qemu-agent-command <vm>
  {"execute":"guest-get-memory-stats"}` only after validating the exact
  custom/downstream capability
- `virsh dumpxml <vm>` (the default for a running domain; `--inactive` is not
  used for live resize validation)
- `virsh qemu-monitor-command <vm> <request>` for the selected device's
  `dynamic-memslots` and `unplugged-inaccessible` properties and QEMU version
- `virsh domxml-to-native qemu-argv --domain <vm>` and `virsh version` for the
  reviewed configuration/version fingerprint
- `virsh update-memory-device <vm> --alias <alias> --requested-size <kib> --live`

The implementation must bound each command, capture a non-zero exit status
with its diagnostic output, and treat it as an explicit failure. Before the
update command, the controller reads and validates fresh XML state, verifies
the configured version-1 attestation fingerprint, and recollects live XML,
native QEMU argv, libvirt/QEMU versions, and alias-selected QMP properties.
It requires an exact match plus `requested == current`. Missing evidence, a
false or empty review field, fingerprint tampering, or configuration/version
drift revokes authorization before `update-memory-device` can run. Review and
attestation files are UTF-8 JSON bounded to 64 KiB with unknown fields denied.
A successful command response does not prove completion: subsequent snapshots
decide convergence. The controller never replays a resize request after a
process restart. The future M10b same-target re-notification operation is the
only planned exception to the ordinary `requested == current` precondition and
must not reuse the general resize entry point.

The same Rust adapters back explicit CLI operations:

- `decision` loads the service environment and uses the configured source,
  freshness checks, alias-scoped XML, and exact runtime evaluator to print one
  read-only policy decision without constructing a resize sink;
- `evidence FILE` reads and validates one M10a2 JSON document without issuing
  any live host or guest command;
- `attest VM ALIAS REVIEW_FILE` performs bounded read-only live collection,
  validates the review JSON, and prints a complete version-1 attestation to
  standard output. It never installs the file or actuates memory;
- `snapshot VM ALIAS` validates that the alias selects exactly one virtio-mem
  device before returning the live domain XML.
- `validate VM ALIAS` reports canonical-byte state and compatibility evidence
  without mutation.
- `resize VM ALIAS TARGET_BYTES` is a dry run unless `--apply` is supplied.
  It requires `--attestation FILE` and a positive host-headroom reserve, then
  reports the exact `virsh` argument vector.

Dry-run and apply share the XML, compatibility, convergence, unit, device-
headroom, and host-headroom checks. Apply executes only the already prepared
argument vector, once, without a shell, retry, or convergence claim.

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
around this lifecycle boundary. An SCM worker that returns successfully
without a stop or shutdown request is classified as an unexpected failure;
SCM receives a non-zero exit code so its bounded recovery actions can run.
Only successful completion after cancellation is an intentional zero-exit
stop.

The SCM adapter emits bounded, single-line Application Event Log records under
source `VirtioMemService`. Stable IDs are `1000` start pending, `1001` running,
`1002` stop requested, `1003` stopped, `2000` configuration failure, `2001`
worker failure, `2002` unexpected worker exit, and `2003` SCM status-publication
failure. Messages are limited to 2,048 Unicode scalar values and do not attach
raw configuration contents. Consumers must currently read the XML `EventData`
insertion string; classic formatted descriptions require a registered message
resource and are not yet part of the contract (ISSUE-008).
