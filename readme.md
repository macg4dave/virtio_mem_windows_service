# virtio-mem Windows controller

This repository is in planning stage and is intentionally limited to Rust and Bash. If a program or service is required, it will be implemented in Rust; Bash remains for local automation and validation helpers.

> **Status:** Planning stage. The repo does not include Go code or Go-based design work.

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

The project avoids Go entirely. Any future runtime component will be Rust-based, and automation will remain Bash-based.

## Planning assumptions

- Use Rust for any long-running service or program logic.
- Use Bash for validation, helper scripts, and local operational tasks.
- Keep the design focused on QEMU Guest Agent interaction and virtio-mem validation.
- Do not add Go toolchains, Go modules, or Go build flows.

## Prerequisites

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