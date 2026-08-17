# Architecture

## System Overview

This system manages dynamic memory allocation for a Windows 11 guest running under QEMU using virtio-mem, with a Linux RHEL controller service.

### Components

- **Linux Controller (Go)**: Polls Windows memory metrics via QEMU Guest Agent, calculates allocation changes, and communicates with libvirt
- **Windows Service**: Deferred. Native QEMU Guest Agent memory statistics are the first integration path; a guest service is added only if validation proves native QGA insufficient.

### Data Flow

```
Windows 11 (Guest)
    ↓ QEMU Guest Agent
    ↓ Unix socket
Linux Controller
    ├── Read memory metrics
    ├── Calculate allocation (2GB hysteresis)
    └── Execute virsh update-memory-device
```

## Service Boundaries

See [copilot-instructions.md](../.github/copilot-instructions.md) for detailed ownership and constraints.

## RPC & Interfaces

- QEMU Guest Agent protocol, accessed by the host through libvirt/`virsh`
- libvirt virtio-mem XML and `update-memory-device` live update
- The controller reads live `requested` and `current` values and waits for convergence before another request

## Controller safety policy

- Poll every 10 seconds by default.
- Grow by 2 GiB when guest available memory is below 2 GiB.
- Shrink by 2 GiB when guest available memory is above 6 GiB.
- Clamp allocation to 8–28 GiB.
- Do not issue a new resize while `requested` and `current` differ.
