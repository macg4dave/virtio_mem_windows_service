# Dependencies

This is the current dependency checklist. Detect installed versions at runtime;
do not copy versions, paths, VM names, or endpoints from prior validation runs.

## Repository toolchain

- Rust with Cargo, rustfmt, and Clippy. The workspace targets Rust 2021 and the
  documented minimum supported Rust version.
- The native Windows MSVC target and linker for the Windows crate.
- Locked Cargo dependencies from the workspace manifests and lockfiles.
- Privileged RHEL workflows use the prebuilt typed xtask boundary; Cargo and
  evidence persistence remain unprivileged.

Run the local dependency and repository checks with:

```bash
cargo xtask doctor host
cargo xtask gate local
```

## Native Windows endpoint

The remote build endpoint requires OpenSSH with key-based access, Rust MSVC,
Visual Studio C++ Build Tools and Windows SDK, archive tooling, and checksum
tooling. Configure the SSH target, absolute remote workspace, local artifact
directory, connection timeout, and operation timeout explicitly as documented
in `testing.md`.

`cargo xtask windows all` is the maintained build/test path. It does not
install the service or alter VM state.

## RHEL/libvirt host

The host requires systemd, libvirt/QEMU, `virsh`, a configured guest-agent
channel, and a virtio-mem device. Live workflows require permission for the
exact domain and device in scope. Record detected libvirt, QEMU, hypervisor,
QGA, driver, and service versions in run evidence and compatibility
attestation; do not encode them in repository procedures.

The controller runs under a dedicated non-login identity with least-privilege
libvirt and filesystem access. Deployment supplies explicit absolute state,
telemetry, and attestation paths.

## Windows guest

The guest requires supported Windows APIs, the virtio-mem driver, QEMU Guest
Agent on its standard virtio-serial channel, and the configured service
identity and ProgramData ACLs. Windows telemetry is native; the service does
not open the QGA channel. The host's production telemetry transport uses the
standard bounded QGA guest-file API to read that protected native record.

The upstream guest agent does not guarantee the optional downstream
`guest-get-memory-stats` command. The host uses validated `dommemstat` by
default and treats alias-scoped live virtio-mem `current` as allocation
authority.

The versioned telemetry contract has capability groups for Windows memory
resource notifications, memory-list detail, and PDH-style paging rates. WN1
publishes those groups as explicitly unavailable without calling new APIs.
WN2 and later may use the Windows-supplied facilities only after the native
capability gate records exact availability, counter names, sampling behavior,
failure behavior, and required privileges. No third-party pressure library is
assumed.

## Live resize prerequisites

Before mutation, resolve and record:

- exact VM and device alias;
- live device size, block geometry, requested, and current;
- reviewed compatibility attestation;
- current deployment floor and host headroom requirement;
- command, sampling, forward-convergence, and rollback time bounds; and
- initial state plus rollback target.

Use `cargo xtask live-resize`; do not reproduce the procedure manually.

See `testing.md` for commands, `architecture.md` for ownership, and
`upstream-virtio-mem-audit.md` for source-backed compatibility constraints.
