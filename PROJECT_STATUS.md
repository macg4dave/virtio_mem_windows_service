# Project Status & Next Steps

**Updated:** 2026-09-05
**Phase:** Phase 2 — Core Functionality
**Overall status:** Windows and host service lifecycles plus single-VM host
actuation are live validated. Trustworthy demand publication, cross-layer
state mapping, host-stat freshness, complete compatibility attestation, and
recovery hardening remain. `win11_gpu` is a fully trusted development/test KVM
guest; upstream Windows virtio-mem support remains technology preview.

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

- Implement the M10c host-side join: publish a fresh raw Windows telemetry
  envelope, join it with alias-scoped live libvirt `current`, and calculate
  the target on the host without adding guest actuation. Production currently
  runs `NativeTelemetryWorker` and discards each validated sample.
- Add the M10d report envelope and delivery contract: VM/service/session
  identity, timestamps and sequence, allocation provenance, freshness/replay
  rules, partial-record handling, ACLs, and retention/rotation.
- Bind the current static workload-review authorization to a live
  domain/QEMU configuration fingerprint in M9d, expanding the attestation to
  all audited backend, memory-slot, VFIO, vDPA/RDMA/vhost-user, balloon,
  secure-virtualization, topology, and version constraints.
- Complete M9e host-telemetry correctness and freshness: fix `dommemstat`
  balloon semantics, validate `last-update`, and replace the QGA-only Bash
  decision preview with the controller's Rust source path.
- Provision ProgramData/configuration ACLs and package a classic Event Log
  message resource; SCM lifecycle/recovery and raw XML EventData are verified.
- Complete M10a1 capture qualification, M10a2 evidence harness, M10a3
  one-block mapping, M10a4 contract adoption, and the M10b failure/recovery
  matrix, including Windows shrink retry/recovery qualification and a
  default-off automatic-shrink control. Use M10aX
  only if bounded kernel-debug capture is not viable.

## External blockers

- `guest-get-memory-stats` is absent from upstream QGA schemas; upgrading an
  upstream QGA is not a remedy. The custom adapter remains experimental and
  the host controller uses `dommemstat` by default.
- `dommemstat actual` is balloon state, not whole-guest or virtio-mem
  allocation, and source freshness is not checked yet. M9e is required before
  this telemetry is considered production-qualified.
- Live resize remains gated by fresh XML validation and
  `requested == current` convergence at the time of each request.
- Automatic Windows shrink is unqualified, and Phase 2 supports only one
  active controller/device on this development host until M10b and M11.
- A hard QEMU/libvirt cgroup memory limit is recommended defense-in-depth for
  trusted `win11_gpu`; it is mandatory for future untrusted or production
  deployments.
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
- [docs/upstream-virtio-mem-audit.md](docs/upstream-virtio-mem-audit.md)

Status is maintained alongside `BACKLOG.md` after each implementation session.
<!-- End of status document. -->
