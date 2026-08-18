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
  "execute": "guest-get-memory-stats"
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
`stat-total` are rejected as inconsistent.

## Memory Change Request

Input: Target memory size in bytes, aligned to the device block size.
Process: `virsh update-memory-device <vm> --alias <virtio-mem-alias> --requested-size <bytes> --live`

The controller must inspect live virtio-mem XML after every request. `requested` is the desired size and `current` is the size currently active in the guest; they may differ while QEMU converges.

### Host-side virtio-mem XML contract

The official libvirt/QEMU model treats virtio-mem as a NUMA-aware memory balloon that is resized by changing the live `requested` value, not by hotplugging a new device. The live XML exposes four relevant values for each memory device:

- `size`: maximum memory the device can currently expose to the guest
- `block`: hotplug granularity; it must be a power of two and larger than 1 MiB in normal use
- `requested`: desired memory exposure for the guest
- `current`: actual memory currently in use by the guest

`requested` must be an integer multiple of `block` and must never exceed `size`. `current` may lag behind `requested` while the guest reclaims or plugs blocks; the controller must treat `requested != current` as an in-flight resize and avoid issuing another change until the guest settles.

When more than one virtio-mem device is present, `virsh` must be directed with `--alias` because the update API cannot infer which device should be resized. The host-side controller should therefore treat the alias as part of the contract and should validate the live XML against the selected alias after each request.

This is a key operational difference from a DIMM or balloon model: virtio-mem is not a simple single-step memory resize, and guest cooperation is required to unplug or plug memory blocks safely.

The pure `parse_virtio_mem_xml` adapter accepts a captured libvirt XML
snapshot, requires the `virtio-mem` model and alias, converts `B`, `KiB`,
`MiB`, and `GiB` values to canonical bytes with checked arithmetic, and
constructs a validated `VirtioMemState`. It performs no host command execution
or live XML discovery.

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
- `virsh dumpxml --live <vm>`
- `virsh update-memory-device <vm> --alias <alias> --requested-size <bytes> --live`

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
