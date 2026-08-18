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

- 72 tests
- `cargo build --workspace --all-features --release`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `bash -n scripts/*.sh`

## Open implementation work

- Wire `windows/src/main.rs` to the configured QGA client, demand worker, and a
  trustworthy current-allocation provider.
- Enforce a configured QGA operation deadline with native overlapped
  connect/write/read and `CancelIoEx` cancellation; synchronous flush is
  intentionally avoided because it has no cancellable deadline.
- Provision ProgramData and service ACLs, event-log output, and real SCM
  installation/recovery behavior.
- Complete host compatibility checks, live systemd validation, and reversible
  resize evidence.

## External blockers

- The attached Windows QGA reports that `guest-get-memory-stats` is unavailable.
- `dommemstat` field availability requires operator verification on the guest.
- The previous `win11_gpu` rollback did not converge within the documented
  timeout; no further live resize should be attempted until it is understood.
- Driver `plugged_size` versus libvirt `current` remains an unverified
  cross-layer mapping.

## References

- [BACKLOG.md](BACKLOG.md) — execution source of truth
- [docs/roadmap.md](docs/roadmap.md) — milestone and phase status
- [docs/architecture.md](docs/architecture.md)
- [docs/feature-matrix.md](docs/feature-matrix.md)
- [docs/testing.md](docs/testing.md)

Status is maintained alongside `BACKLOG.md` after each implementation session.
<!-- End of status document. -->