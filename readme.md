# virtio-mem Windows controller

Go controller for dynamically adjusting the virtio-mem allocation of a Windows 11 KVM guest running on a RHEL host with libvirt and QEMU.

The controller runs on the **RHEL host**. It reads Windows memory statistics through the QEMU Guest Agent, reads the live virtio-mem state from libvirt XML, and requests safe live memory changes through `virsh`.

> **Status:** The Go controller foundation is implemented and unit-tested. QEMU Guest Agent installation and live resize behavior still need validation on the target RHEL 10.2 host and Windows 11 VM. The Rust Windows service is deferred unless native QEMU Guest Agent statistics prove insufficient.

## Architecture

```text
Windows 11 guest
	└─ QEMU Guest Agent
			 ▲
			 │ virtio-serial channel / QGA
RHEL 10.2 host
	└─ virtio-mem controller
			 ├─ virsh qemu-agent-command
			 ├─ live virtio-mem XML inspection
			 └─ virsh update-memory-device --live
```

The controller does not access the Windows registry or filesystem and does not execute guest shell commands as part of its normal polling loop.

## Memory policy

Defaults are conservative and can be changed with command-line flags:

| Setting | Default |
|---|---:|
| Poll interval | 10 seconds |
| Minimum allocation | 8 GiB |
| Maximum allocation | 28 GiB |
| Resize step | 2 GiB |
| Grow below available memory | 2 GiB |
| Shrink above available memory | 6 GiB |

Safety rules:

- The controller reads live virtio-mem `current` and `requested` values.
- It does not issue another resize while the previous request is converging.
- Targets are clamped to the configured minimum and maximum.
- QEMU Guest Agent and libvirt failures are logged and skipped for that poll cycle.

## Prerequisites

### RHEL host

- RHEL 10.2 with a working libvirt-managed Windows VM
- QEMU and libvirt with virtio-mem support
- `virsh` available to the account running the controller
- Go 1.20 or newer for building
- `jq` for optional manual response inspection
- A configured virtio-mem device with a known alias

The eventual systemd service should use a dedicated least-privilege account and the correct system libvirt connection. Do not solve permission problems by granting unrestricted `sudo` or disabling SELinux.

### Windows 11 guest

- Windows 11 running under QEMU/KVM
- x64 QEMU Guest Agent installed and running
- A virtio-serial channel named `org.qemu.guest_agent.0`

Follow [`docs/qemu-ga-setup.md`](docs/qemu-ga-setup.md) to install and validate the guest agent.