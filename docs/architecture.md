# Architecture

## System Overview

This system manages dynamic memory allocation for a Windows 11 guest running under QEMU using virtio-mem. The repository is intentionally scoped to Rust and Bash only; any runtime service will be implemented as a Rust program rather than a Go controller.

The validated `win11_gpu` instance is a fully trusted development/test KVM
guest. Upstream Windows virtio-mem support is technology preview; production
and untrusted-guest operation are outside the current support boundary.

### Components

- **Windows Service (Rust)**: Guest-side native memory telemetry, advisory demand calculation/publication, cancellation, and service lifecycle hosting
- **RHEL host controller (Rust/systemd)**: One explicitly configured VM and virtio-mem alias per unit instance; reads QGA and live libvirt state, then issues validated live resize requests
- **Host validation / automation (Bash)**: Explicit preflight, diagnostic, and manual operational helpers; these do not run inside the controller
- **QEMU / libvirt validation path**: Used to verify guest agent responses and live virtio-mem behavior

### Data Flow

```text
Windows demand agent ── future raw telemetry envelope ─────► Host policy
QEMU Guest Agent ── virtio-serial/libvirt ─────────────────► Host health adapter
Live libvirt/QEMU state ───────────────────────────────────► Host controller
Host controller ── validated aligned request ──────────────► virtio-mem device
```

### Phase 2 and Phase 3 ownership

Phase 2 is a transition architecture. The Windows service measures guest
memory and will publish a fresh raw telemetry envelope. The one-VM RHEL host
joins that envelope with alias-scoped live libvirt `current`, calculates the
recommendation, and remains the allocation and actuation authority. The
Windows service does not invoke Linux commands, modify libvirt, receive a host
allocation feed, or directly control `viomem.sys`.

The Phase 3 target separates the system into three cooperating layers:

1. **Windows demand agent:** collects and publishes fresh, versioned native
    Windows telemetry and may classify guest-local pressure.
2. **Per-VM QEMU/libvirt adapter:** validates an aligned target, changes
    virtio-mem `requested`, and observes asynchronous `current` convergence.
3. **Linux global controller:** owns host reserve, VM pool accounting, and
    multi-VM growth/reclaim arbitration.

See [`future-architecture.md`](future-architecture.md) for the target design.
Multi-VM arbitration and global pool ownership are not implemented by the
current Phase 2 controller.

Although upstream QEMU/libvirt can model multiple virtio-mem devices, Phase 2
supports only one active controller managing one explicitly named VM/device on
this development host. Separate per-instance host-reserve checks do not
provide global reservation atomicity. Multi-controller/device actuation is
unsupported until M11 owns the host pool.

## RHEL host controller lifecycle and boundaries

The RHEL controller is a Rust process supervised by a templated systemd unit.
Each unit instance owns one explicitly configured VM name and virtio-mem alias;
it must not enumerate domains or manage multiple VMs through an implicit
configuration. Its only host integration is bounded, argument-safe `virsh`
subprocess calls for QGA statistics, live XML snapshots, live QMP compatibility
properties, and approved live resize requests. It never invokes a shell or
administers Windows processes.

The controller uses the same byte-based state and resize policy as the Windows
service. Before a resize, it validates the selected live XML state and target,
reads `dynamic-memslots` and `unplugged-inaccessible` from the selected live
QOM device, and validates a separately recorded version-1 JSON attestation.
Its SHA-256 fingerprint binds review declarations to fresh alias-scoped domain
XML, native QEMU arguments, QMP properties/QEMU version, and libvirt version.
Those live inputs cover the selected backend, NUMA/page properties, slot/VFIO
topology, balloon, and incompatible device configuration; only changing
virtio-mem `requested`/`current` values are scrubbed. Bound declarations record
slot and VFIO budgets, trusted-development classification, Windows driver
version, and vDPA/RDMA/VFIO-NVMe/`mlock`/secure-virtualization/vhost-user/
balloon exclusions. The service account reads but must not modify this file.
Every resize fails closed on missing, malformed, tampered, unsupported-version,
or drifted evidence.
After a request, it waits for `requested` and `current` to converge and never
sends a follow-up request while they differ. Invalid configuration, failed QGA
calls, malformed XML, failed resize commands, and convergence timeouts are
actionable failures; a bounded systemd restart must reread live state rather
than replay a previous request.

### Measurement, policy, and actuation

These concerns are intentionally separate:

- **Measurement** observes Windows and host state.
- **Policy** produces a recommendation or global allocation decision.
- **Actuation** changes virtio-mem and reports whether the guest converged.

The Phase 2 Windows service owns guest measurement only. The host owns the
M10c join, recommendation, allocation decision, and resize request. A
future global Linux controller will own cross-VM policy.

## Service Boundaries

See [copilot-instructions.md](../.github/copilot-instructions.md) for detailed ownership and constraints.

## Windows service lifecycle and operations

The Rust runtime must treat the Windows Service Control Manager (SCM) as a
lifecycle coordinator, not as the worker loop itself. SCM callbacks should do
only bounded setup or shutdown coordination and return promptly; native Windows
telemetry belongs to the stoppable background runtime. QEMU Guest Agent
requests are owned by the RHEL/libvirt host controller because the QGA process
owns the Windows virtio-serial device.

The SCM adapter must make lifecycle transitions observable and deterministic:

- report start-pending before initialization, then running only after the
    worker is ready;
- honor stop and system-shutdown requests by signaling cancellation, stopping
    new polls, and waiting for the worker to exit cleanly;
- report stop-pending when shutdown may exceed the immediate callback window,
    then stopped on successful completion;
- distinguish an expected cancellation or operator stop from an unexpected
    worker failure; and
- return a non-zero process result for unexpected terminal failures so SCM
    recovery actions can operate, while normal stops remain successful.

Startup configuration must be small, documented, validated, and safe to
override. Persistent settings belong in the service's configuration mechanism
rather than undocumented command-line arguments. The service registration
must define a stable service name, display name, description, executable path,
startup mode, and an explicitly chosen account. Use the least-privileged
account that can collect native telemetry, publish to approved ProgramData
paths, and emit Event Log records; do not default to LocalSystem without a
documented requirement.

Installation, recovery configuration, start/stop verification, event-log
inspection, and removal are operational procedures and must be reproducible
from the repository documentation. Recovery actions should be configured only
after distinguishing crash/failure exits from intentional stops, and should
use bounded restart delays to avoid a tight restart loop.

These rules are adapted from [Microsoft's Windows service walkthrough](https://learn.microsoft.com/en-us/dotnet/framework/windows-services/walkthrough-creating-a-windows-service-application-in-the-component-designer)
and its [current Windows service guidance](https://learn.microsoft.com/en-us/dotnet/core/extensions/windows-service); the implementation remains Rust-only.

The current Rust implementation provides `ServiceHost`, `StopSignal`, a
wakeable raw-telemetry publication loop, validated `ServiceConfig` defaults, a
native SCM callback/registration adapter, installation/start/stop/removal
commands, the pure Rust `VirtioMemState` byte/alignment validator, a versioned
JSON configuration loader, and a generic `DemandServiceWorker` that publishes
advisory reports through an injected JSON-lines sink. The SCM path emits
bounded lifecycle and failure records to the Windows Application Event Log
with stable event IDs; raw XML EventData and recovery behavior are verified
live. Production runs `RawTelemetryWorker`, which publishes VM-scoped,
wall-clock-stamped raw counters without allocation input. The host validates
freshness and VM identity, joins the record with alias-scoped live libvirt
`current`, and calculates the target through shared policy. M10d has added
version-2 producer identity, bounded atomic handoff/retention, durable
restart-safe replay validation, and ProgramData ACL provisioning. Installed
ACL verification remains. No Windows production resize sink is permitted. The QGA named-pipe client is
retained as an explicit adapter/test boundary, but the SCM worker does not
open the QGA virtio-serial device; the host controller owns QGA requests.
Interactive and SCM startup use the same native telemetry worker boundary and
preserve stage-specific runtime-wiring context for configuration validation,
worker construction/initialization, and service-host failures. No guest-side
resize sink is constructed by this path.

## RPC & Interfaces

- QEMU Guest Agent protocol accessed through libvirt / `virsh`
- libvirt virtio-mem XML inspection for validation and live adjustment checks
- Future runtime logic will remain in Rust, never in Go

## Windows virtio-mem driver boundary

The upstream `viomem.sys` driver owns block-level memory mechanics. Its source
maintains a block bitmap, distinguishes `requested_size` from `plugged_size`,
supports `VIRTIO_MEM_F_ACPI_PXM` and
`VIRTIO_MEM_F_UNPLUGGED_INACCESSIBLE`, adds memory with
`MmAddPhysicalMemory`, and uses `MmAllocateNodePagesForMdlEx` with
`MM_ALLOCATE_AND_HOT_REMOVE` for removal.

The Windows service must not duplicate page selection or assume that it can
unplug arbitrary memory. A supported user-mode IOCTL/status API has not been
established, so direct driver communication is deferred. The Virtio 1.2
device contract and the pinned QEMU/libvirt and virtio-win sources establish
that driver `requested_size`/`plugged_size` are the device-side forms of the
requested and plugged allocation represented by host `requested`/`current`.
The host contract therefore uses alias-scoped live libvirt `current` as its
allocation authority; it does not consume a duplicate guest-side state feed.

The upstream driver is built as a KMDF/Visual Studio solution with separate
VirtIO/WDF library dependencies and Win10/Win11 architecture configurations.
This repository does not build, install, sign, or modify that kernel driver.
Any driver fork or added status interface requires its own signing, security,
installation, rollback, and live-validation plan. The completed M10aX
[feasibility proposal](driver-status-interface-feasibility.md) defines those
gates for a cached read-only diagnostic snapshot while leaving implementation
deferred.

Read-only inspection of the signed `100.102.104.29400` driver confirms that
PnP properties, registry parameters, Event Log channels, and registered trace
providers do not expose `requested_size` or `plugged_size`. The matching
upstream source formats both values only for kernel debug output: its WPP build
switch is disabled and it defines no I/O queue/device-control callback. A
kernel-debug capture is therefore an operational mutation of the protected
guest, not a normal service API, and requires its own approval and rollback
procedure. Such capture is optional diagnostic evidence for the installed
binary and becomes important when explaining notification timing, branch
selection, or a no-progress shrink. Its absence does not block host allocation
accounting or hermetic global-pool simulation.

## Safety policy

- Keep validation conservative and explicit.
- Confirm QEMU Guest Agent responses before enabling automated changes.
- Avoid speculative memory changes without a successful behavior check.
- Preserve a clear separation between guest-side logic and host-side automation.
- A resize target may never leave less than `MIN_HEADROOM_BYTES` (1 GiB) of a
    virtio-mem device's declared size unplugged; this is enforced in the
    shared `VirtioMemState::validate_target` contract, not only by operator
    configuration.
- The RHEL host controller must confirm host-side `MemAvailable` covers a grow
    request plus a configured reserve (`VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES`)
    before sending it; insufficient headroom blocks the request for that
    poll cycle instead of failing the service.
- Per-VM actuation is asymmetric: growth advances by at most 1 GiB per
    converged request, while reclaim advances by at most 64 MiB. Both quanta
    are block-aligned, clamped to configured limits, and suppressed while
    `requested != current`.
- When the connected QEMU Guest Agent does not implement the nonstandard
    `guest-get-memory-stats` extension, the host controller uses
    `virsh dommemstat`. Balloon `actual` remains provenance rather than an
    allocation/total bound; required libvirt `last_update` must be recent, within the
    future-skew allowance, and advance between controller samples.
- QEMU does not completely prevent guest access to unplugged memory. A hard
    QEMU/libvirt cgroup memory limit is recommended defense-in-depth for fully
    trusted development guest `win11_gpu` and mandatory for untrusted or
    production deployments.
- Automatic Windows shrinking remains disabled after a live 64 MiB request
    made no progress. M10b's bounded retry/re-notification/recovery is retained
    as diagnostic and operator-recovery behavior. M10e-M10g must add and
    qualify an absolute target plus desired/requested/current reconciliation
    before automated reclaim is supported.
