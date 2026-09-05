# Project Status & Next Steps

**Updated:** 2026-09-05
**Phase:** Phase 2 — Core Functionality
**Overall status:** Windows and host service lifecycles plus single-VM host
actuation are live validated. Trustworthy demand publication, cross-layer
state mapping, configuration-drift protection, and recovery hardening remain.

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

The latest native RHEL gate passed:

- 22 shared-core and 29 host tests, with no failures.
- `cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked`
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

The RHEL host-controller artifact was rebuilt and installed on 2026-09-05 at
`/usr/local/libexec/virtio-mem-host`; 22 core and 29 host tests passed. The
enabled `virtio-mem-host@win11_gpu.service` completed a guarded zero-to-1-GiB
bootstrap, converged, retained its minimum, and remains active. The verified
Windows service artifact was built on the Win11 guest and fetched to
`.vscode-artifacts/windows/virtio-mem-service.exe`.

## Open implementation work

- Decide current-allocation ownership in M10c, then wire native telemetry to a
  demand publisher without adding guest actuation. Production currently runs
  `NativeTelemetryWorker` and discards each validated sample.
- Add the M10d report envelope and delivery contract: VM/service/session
  identity, timestamps and sequence, allocation provenance, freshness/replay
  rules, partial-record handling, ACLs, and retention/rotation.
- Bind the current static workload-review authorization to a live
  domain/QEMU configuration fingerprint in M9d.
- Provision ProgramData/configuration ACLs and package a classic Event Log
  message resource; SCM lifecycle/recovery and raw XML EventData are verified.
- Complete M10a1 capture qualification, M10a2 evidence harness, M10a3
  one-block mapping, M10a4 contract adoption, and the M10b failure/recovery
  matrix. Use M10aX
  only if bounded kernel-debug capture is not viable.

## External blockers

- The attached Windows QGA does not advertise `guest-get-memory-stats`; the
  host controller uses the verified `dommemstat` fallback by default.
- Live resize remains gated by fresh XML validation and
  `requested == current` convergence at the time of each request.
- Driver `plugged_size` versus libvirt `current` remains an unverified
  cross-layer mapping.
- Live read-only inspection of signed `viomem.sys` `100.102.104.29400` found
  no supported user-mode state query. The upstream state record is available
  only through kernel debug output, so M10a requires separate approval for a
  bounded protected-guest capture and reversible resize (or a separate signed-
  driver interface project).

The previous live convergence incident was resolved on 2026-08-18 at zero.
That evidence is historical: M9b subsequently bootstrapped and converged the
same device at 1 GiB. Neither state proves direct driver-field mapping.

## References

- [BACKLOG.md](BACKLOG.md) — execution source of truth
- [docs/roadmap.md](docs/roadmap.md) — milestone and phase status
- [docs/architecture.md](docs/architecture.md)
- [docs/feature-matrix.md](docs/feature-matrix.md)
- [docs/testing.md](docs/testing.md)

Status is maintained alongside `BACKLOG.md` after each implementation session.
<!-- End of status document. -->
