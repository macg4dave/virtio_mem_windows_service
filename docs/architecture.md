# Architecture

## System Overview

This system manages dynamic memory allocation for a Windows 11 guest running under QEMU using virtio-mem. The repository is intentionally scoped to Rust and Bash only; any runtime service will be implemented as a Rust program rather than a Go controller.

### Components

- **Windows Service (Rust)**: Guest-side demand agent for memory telemetry, demand calculation, QEMU Guest Agent compatibility, cancellation, and service lifecycle hosting
- **RHEL host controller (Rust/systemd)**: One explicitly configured VM and virtio-mem alias per unit instance; reads QGA and live libvirt state, then issues validated live resize requests
- **Host validation / automation (Bash)**: Explicit preflight, diagnostic, and manual operational helpers; these do not run inside the controller
- **QEMU / libvirt validation path**: Used to verify guest agent responses and live virtio-mem behavior

### Data Flow

```text
Windows 11 (Guest)
    ↓ QEMU Guest Agent
    ↓ Unix socket / libvirt interface
Host validation and runtime tooling
    ├── Read memory metrics
    ├── Validate guest-agent behavior
    └── Coordinate virtio-mem verification
```

### Phase 2 and Phase 3 ownership

Phase 2 is a transition architecture. The Windows service may measure guest
memory and report a demand recommendation, but the existing one-VM RHEL host
controller remains the allocation and actuation authority. The Windows service
does not invoke Linux commands, modify libvirt, or directly control
`viomem.sys`.

The Phase 3 target separates the system into three cooperating layers:

1. **Windows demand agent:** collects native Windows telemetry and reports raw
    measurements, pressure, demand state, desired target, and safe-floor
    recommendation.
2. **Per-VM QEMU/libvirt adapter:** validates an aligned target, changes
    virtio-mem `requested`, and observes asynchronous `current` convergence.
3. **Linux global controller:** owns host reserve, VM pool accounting, and
    multi-VM growth/reclaim arbitration.

See [`future-architecture.md`](future-architecture.md) for the target design.
Multi-VM arbitration and global pool ownership are not implemented by the
current Phase 2 controller.

## RHEL host controller lifecycle and boundaries

The RHEL controller is a Rust process supervised by a templated systemd unit.
Each unit instance owns one explicitly configured VM name and virtio-mem alias;
it must not enumerate domains or manage multiple VMs through an implicit
configuration. Its only host integration is bounded, argument-safe `virsh`
subprocess calls for QGA statistics, live XML snapshots, and approved live
resize requests. It never invokes a shell or administers Windows processes.

The controller uses the same byte-based state and resize policy as the Windows
service. Before a resize, it validates the selected live XML state and target.
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

The Phase 2 Windows service owns guest measurement and recommendation only. The
host controller owns the currently implemented allocation decision and resize
request. A future global Linux controller will own cross-VM policy.

## Service Boundaries

See [copilot-instructions.md](../.github/copilot-instructions.md) for detailed ownership and constraints.

## Windows service lifecycle and operations

The Rust runtime must treat the Windows Service Control Manager (SCM) as a
lifecycle coordinator, not as the worker loop itself. SCM callbacks should do
only bounded setup or shutdown coordination and return promptly; polling and
QEMU Guest Agent work belong to the stoppable background runtime.

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
account that can access the QEMU Guest Agent channel; do not default to
LocalSystem without a documented requirement.

Installation, recovery configuration, start/stop verification, event-log
inspection, and removal are operational procedures and must be reproducible
from the repository documentation. Recovery actions should be configured only
after distinguishing crash/failure exits from intentional stops, and should
use bounded restart delays to avoid a tight restart loop.

These rules are adapted from [Microsoft's Windows service walkthrough](https://learn.microsoft.com/en-us/dotnet/framework/windows-services/walkthrough-creating-a-windows-service-application-in-the-component-designer)
and its [current Windows service guidance](https://learn.microsoft.com/en-us/dotnet/core/extensions/windows-service); the implementation remains Rust-only.

The current Rust implementation provides `ServiceHost`, `StopSignal`, a
wakeable polling loop, validated `ServiceConfig` defaults, a native SCM
callback/registration adapter, installation/start/stop/removal commands, and
the pure Rust `VirtioMemState` byte/alignment validator. Persistent
configuration loading, event-log integration, live XML parsing, and the
production resize sink remain to be implemented.

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
established, so direct driver communication is deferred. The relationship
between driver `plugged_size` and libvirt `current` requires live validation
before it becomes a shared accounting contract.

The upstream driver is built as a KMDF/Visual Studio solution with separate
VirtIO/WDF library dependencies and Win10/Win11 architecture configurations.
This repository does not build, install, sign, or modify that kernel driver.
Any driver fork or added status interface requires its own signing, security,
installation, rollback, and live-validation plan.

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
- When the connected QEMU Guest Agent does not implement
    `guest-get-memory-stats`, the host controller must use an alternative
    memory-stat source (`virsh dommemstat`) rather than proceed without
    metrics; see `docs/api-contract.md`.
