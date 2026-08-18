# virtio-mem Windows controller

This repository is in implementation and is intentionally limited to Rust and
Bash. Runtime components are Rust; Bash remains for local automation and
validation helpers.

> **Status:** Phase 2 foundation. The shared Rust policy core, Windows service
> foundation, and RHEL systemd controller are implemented locally; live KVM
> validation remains blocked on a RHEL/libvirt host and Windows guest.

## Architecture

```text
Windows 11 guest
    └─ QEMU Guest Agent
            ▲
            │ virtio-serial channel / QGA
RHEL host
    └─ virtio-mem orchestration
            ├─ libvirt / QEMU validation
            ├─ runtime monitoring
            └─ operational automation
```

The project avoids Go entirely. The Windows service and RHEL controller are
Rust-based; automation remains Bash-based.

## Planning assumptions

- Use Rust for any long-running service or program logic.
- Use Bash for validation, helper scripts, and local operational tasks.
- Keep the design focused on QEMU Guest Agent interaction and virtio-mem validation.
- Do not add Go toolchains, Go modules, or Go build flows.

## Prerequisites

See the complete dependency and requirement checklist in
[`docs/dependencies.md`](docs/dependencies.md).

### Host / environment

- RHEL host with libvirt and QEMU available
- Windows 11 guest VM with QEMU Guest Agent installed and running
- `virsh` available for validation and inspection
- `jq` for optional manual response inspection
- A configured virtio-mem device and known alias for the target VM

### Windows 11 guest

- Windows 11 running under QEMU/KVM
- x64 QEMU Guest Agent installed and running
- A virtio-serial channel named `org.qemu.guest_agent.0`

Follow [`docs/qemu-ga-setup.md`](docs/qemu-ga-setup.md) to install and validate the guest agent. This repo does not use Go for the runtime implementation.

## Host validation helpers

From a RHEL host, run `bash scripts/check-environment.sh` before
`bash scripts/validate-guest-agent.sh VM_NAME 3`. The validation helper only
queries the explicitly supplied VM and never changes VM memory or executes
guest commands.

## RHEL host controller

`host/` provides `virtio-mem-host`, a Rust process supervised by the templated
`host/systemd/virtio-mem-host@.service` unit. Each instance manages one
explicitly configured VM and virtio-mem alias. It reads QEMU Guest Agent
statistics and live XML through bounded `virsh` calls, validates byte-aligned
requests, and does not issue another resize until the previous one converges.

Build all Rust components with `bash scripts/build-rust.sh`. Deployment and
live-validation requirements are documented in `docs/dependencies.md` and
`docs/testing.md`; do not enable the systemd unit before the manual safety gate
has succeeded.
