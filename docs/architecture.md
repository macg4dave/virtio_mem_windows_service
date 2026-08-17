# Architecture

## System Overview

This system manages dynamic memory allocation for a Windows 11 guest running under QEMU using virtio-mem. The repository is intentionally scoped to Rust and Bash only; any runtime service will be implemented as a Rust program rather than a Go controller.

### Components

- **Windows Service (Rust)**: Planned guest-side runtime for memory metrics and QEMU Guest Agent interaction
- **Host validation / automation (Bash)**: Planned scripts for validation, local build workflow, and operational checks
- **QEMU / libvirt validation path**: Used to verify guest agent responses and live virtio-mem behavior

### Data Flow

```
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

## RPC & Interfaces

- QEMU Guest Agent protocol accessed through libvirt / `virsh`
- libvirt virtio-mem XML inspection for validation and live adjustment checks
- Future runtime logic will remain in Rust, never in Go

## Safety policy

- Keep validation conservative and explicit.
- Confirm QEMU Guest Agent responses before enabling automated changes.
- Avoid speculative memory changes without a successful behavior check.
- Preserve a clear separation between guest-side logic and host-side automation.
