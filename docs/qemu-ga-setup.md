# QEMU Guest Agent setup and validation

QGA is a host-owned health and compatibility interface. The Windows telemetry
service does not open its virtio-serial channel and does not depend on a custom
memory-stats command.

## Required state

- The selected guest has QEMU Guest Agent installed and running.
- Domain configuration exposes the standard QGA virtio-serial channel.
- The libvirt identity used by validation can access only the intended domain.
- Any installation, domain-definition change, service restart, or guest reboot
  follows the repository approval and deployment process.

Installation and channel configuration are platform-administrator tasks; use
the current QEMU/libvirt and guest-agent documentation for the installed
versions. Do not copy repository-era package paths or VM-specific commands.

## Read-only validation

After setup, validate the explicit target through the maintained Rust tool:

```bash
cargo xtask qga VM_NAME \
  --attempts ATTEMPT_COUNT \
  --command-timeout-seconds COMMAND_BOUND \
  --connect LIBVIRT_URI
```

Success requires valid `guest-info` evidence on every configured attempt. If
the optional downstream memory command is absent, the tool validates required
numeric `dommemstat` fields instead. This proves connectivity and source shape,
not guest application health, freshness across controller cycles, or resize
convergence.

For failures, inspect the recorded command stage, the named guest service, the
live channel definition, and host authorization. Do not reboot, restart, or
edit the VM merely because one probe timed out; those are separate operations
with their own scope and evidence.

See `testing.md` for layered guest-health and live-validation requirements.
