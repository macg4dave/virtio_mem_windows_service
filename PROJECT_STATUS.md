# Project Status & Next Steps

**Updated:** 2026-09-04
**Phase:** Phase 2 — Core Functionality
**Overall status:** Local foundations are implemented and validated; concrete
Windows runtime wiring and live KVM evidence remain open.

## Completed locally

- Architecture, API contracts, data model, engineering standards, testing
  strategy, roadmap, backlog, and QEMU Guest Agent setup documentation.
- Shared byte-based memory policy with alignment, bounds, hysteresis, and
  requested/current convergence protection.
- Windows QGA named-pipe boundary, parser, polling loop, cancellation wake-up,
  portable service lifecycle, validated JSON configuration, and native SCM
  adapter with install/start/stop/remove commands.
- Host-side virtio-mem XML validation, bounded `virsh` adapters, `dommemstat`
  fallback, convergence suppression, device headroom, and host headroom gates.
- Authoritative Rust host CLI for alias-scoped snapshot/validation, exact
  dry-run argument reporting, and explicitly applied one-shot resize; the
  duplicate Bash resize implementation is removed.
- Native Windows Application Event Log lifecycle/failure emission with stable
  IDs, bounded messages, and non-zero recovery semantics for worker failures
  or stopless exits.
- Windows native demand telemetry using `GlobalMemoryStatusEx` and
  `GetPerformanceInfo`, versioned advisory reports, aligned recommendations,
  JSON-lines publication, and a generic stoppable demand worker.

## Current evidence

The 2026-09-04 M9c native RHEL gate passed:

- 44 core/host tests (19 core and 25 host), with no failures.
- `cargo build -p virtio-mem-core -p virtio-mem-host --all-features --release --locked`
- `cargo fmt --all -- --check`
- `cargo clippy -p virtio-mem-core -p virtio-mem-host --all-targets --all-features --locked -- -D warnings`
- `bash -n scripts/*.sh`

The M7 Windows-native gate passes 64 tests with warnings denied and produced a
checksum-verified executable with SHA-256
`2dc1bf9df309e86c39119890cad8c7419de831e3276ba36b920535a72ed26c8e`.

Live M7 validation on `ice101.lan` passed the LocalService
install/start/observe/stop/delete sequence. Event IDs 1000–1003 were observed
for a clean lifecycle with exit code zero. Invalid configuration emitted 2000,
exited with code one, and restarted in 5.05 seconds under the configured
5/30/60-second policy. Rollback restored the original binary hash, recovery
configuration, service security descriptor, absent ProgramData directory, and
running service state.

The RHEL host-controller artifact was rebuilt on 2026-09-04 at
`target/release/virtio-mem-host`; its 25 package tests passed. The verified
Windows service artifact was built on the Win11 guest and fetched to
`.vscode-artifacts/windows/virtio-mem-service.exe`.

## Open implementation work

- Wire `windows/src/main.rs` to the configured QGA client and demand worker;
  trustworthy current-allocation provider and resize sink remain open.
- Confirm the corrected host XML discovery command and complete live systemd
  validation; this host rejects a live option on `dumpxml`, and the controller
  now uses the default `virsh dumpxml <vm>` form.
- Provision ProgramData/configuration ACLs and package a classic Event Log
  message resource; SCM lifecycle/recovery and raw XML EventData are verified.
- Complete host compatibility checks, live systemd validation, and reversible
  resize evidence.

## External blockers

- The attached Windows QGA does not advertise `guest-get-memory-stats`; the
  host controller uses the verified `dommemstat` fallback by default.
- Live resize remains gated by fresh XML validation and
  `requested == current` convergence at the time of each request.
- Driver `plugged_size` versus libvirt `current` remains an unverified
  cross-layer mapping.

The previous live convergence incident is resolved as of 2026-08-18: a fresh
post-driver-update XML check reports `requested=0 KiB` and `current=0 KiB` for
`ua-virtiomem0`. This clears the stale rollback blocker, but does not replace
the required controlled resize evidence or prove direct driver-field mapping.

## References

- [BACKLOG.md](BACKLOG.md) — execution source of truth
- [docs/roadmap.md](docs/roadmap.md) — milestone and phase status
- [docs/architecture.md](docs/architecture.md)
- [docs/feature-matrix.md](docs/feature-matrix.md)
- [docs/testing.md](docs/testing.md)

Status is maintained alongside `BACKLOG.md` after each implementation session.
<!-- End of status document. -->
