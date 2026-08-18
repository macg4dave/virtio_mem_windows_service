# Project Status & Next Steps

**Updated:** 2026-08-18
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
- Windows native demand telemetry using `GlobalMemoryStatusEx` and
  `GetPerformanceInfo`, versioned advisory reports, aligned recommendations,
  JSON-lines publication, and a generic stoppable demand worker.

## Current evidence

The 2026-08-18 local workspace gate passed:

- 77 tests before the latest host-parser regression test (the current count is
  reported by the validation run below).
- `cargo build --workspace --all-features --release`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `bash -n scripts/*.sh`

The RHEL host-controller artifact was rebuilt on 2026-08-18 at
`target/release/virtio-mem-host`; its 14 package tests passed. A Windows
service artifact is not available from this host because the Windows Rust
target and a Windows linker are not installed. The Windows service must be
built on the Win11 guest or another Windows build host.

## Open implementation work

- Wire `windows/src/main.rs` to the configured QGA client and demand worker;
  trustworthy current-allocation provider and resize sink remain open.
- Confirm the corrected host XML discovery command and complete live systemd
  validation; this host rejects a live option on `dumpxml`, and the controller
  now uses the default `virsh dumpxml <vm>` form.
- Provision ProgramData and service ACLs, event-log output, and real SCM
  installation/recovery behavior.
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