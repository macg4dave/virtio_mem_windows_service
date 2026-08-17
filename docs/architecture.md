# Architecture

## System Overview

This system manages dynamic memory allocation for a Windows 11 guest running under QEMU using virtio-mem. The repository is intentionally scoped to Rust and Bash only; any runtime service will be implemented as a Rust program rather than a Go controller.

### Components

- **Windows Service (Rust)**: Guest-side runtime for memory metrics, QEMU Guest Agent interaction, cancellation, and service lifecycle hosting
- **Host validation / automation (Bash)**: Planned scripts for validation, local build workflow, and operational checks
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
wakeable polling loop, and validated `ServiceConfig` defaults. The native SCM
callback/registration adapter, persistent configuration loading, event-log
sink, and installation/recovery commands remain to be implemented.

## RPC & Interfaces

- QEMU Guest Agent protocol accessed through libvirt / `virsh`
- libvirt virtio-mem XML inspection for validation and live adjustment checks
- Future runtime logic will remain in Rust, never in Go

## Safety policy

- Keep validation conservative and explicit.
- Confirm QEMU Guest Agent responses before enabling automated changes.
- Avoid speculative memory changes without a successful behavior check.
- Preserve a clear separation between guest-side logic and host-side automation.
