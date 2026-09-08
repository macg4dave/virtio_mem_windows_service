# Project Status & Next Steps

**Updated:** 2026-09-08
**Phase:** Phase 2 — Core Functionality
**Overall status:** Windows and host service lifecycles plus single-VM host
actuation are live validated. The host-side demand join is locally complete;
bounded delivery and recovery hardening remain. Host-stat freshness and the complete compatibility
attestation is implemented and awaits a separately approved live installation.
The allocation-authority contract is established from Virtio and pinned
implementation sources; optional driver tracing remains diagnostic.
`win11_gpu` is a fully trusted development/test KVM
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
- Freshness-qualified `dommemstat` snapshots with balloon-correct semantics,
  configured age/skew bounds, strict advancement, and a Rust decision preview
  sharing the controller evaluator.
- Authoritative Rust host CLI for alias-scoped snapshot/validation, exact
  dry-run argument reporting, and explicitly applied one-shot resize; the
  duplicate Bash resize implementation is removed.
- Version-1 SHA-256 compatibility attestation and read-only generation command;
  every resize checks integrity and fresh allocation-neutral live
  domain/QEMU/QMP/version evidence before actuation.
- Native Windows Application Event Log lifecycle/failure emission with stable
  IDs, bounded messages, and non-zero recovery semantics for worker failures
  or stopless exits.
- Windows native demand telemetry using `GlobalMemoryStatusEx` and
  `GetPerformanceInfo`, versioned advisory reports, aligned recommendations,
  JSON-lines publication, and a generic stoppable demand worker.
- Production Windows raw telemetry publication with VM/time fields, plus host
  freshness/identity validation and target calculation joined to fresh
  alias-scoped live libvirt `current`.

## Current evidence

The latest native RHEL gate passed:

- 31 shared-core and 40 host tests, with no failures.
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
bootstrap, converged, and retained its minimum. A bounded 2026-09-07 M10a3
operation then proved one-block growth but a no-progress Windows shrink:
`requested` returned to 1 GiB while `current` remained 1 GiB + 2 MiB for 300
seconds. The controller was intentionally stopped pending domain-restart
recovery. That recovery completed through graceful QGA shutdown/start;
the device is again converged at 1 GiB and the controller is active with zero
restarts. A follow-up 3 GiB-to-2 GiB probe unplugged 257 blocks (514 MiB),
then left 255 blocks (510 MiB) above target for 300 seconds. Its recovery also
restored the same healthy 1 GiB baseline. The verified Windows service artifact
was built on the Win11 guest and fetched to
`.vscode-artifacts/windows/virtio-mem-service.exe`.

## Open implementation work

- Add the M10d report envelope and delivery contract: VM/service/session
  identity beyond the M10c VM name, monotonic/session sequence, allocation
  provenance, replay rules, partial/oversized-record handling, ACLs, bounded
  reader handoff, and retention/rotation.
- Install a freshly reviewed M9d attestation with read-only service-account
  access before deploying this build. The implemented version-1 SHA-256 guard
  binds backend, memory-slot, VFIO, incompatible-workload, balloon, topology,
  trust, driver, QEMU, and libvirt evidence and rejects drift before resize.
- Provision ProgramData/configuration ACLs and package a classic Event Log
  message resource; SCM lifecycle/recovery and raw XML EventData are verified.
- Complete the M10a2 correlated behavior-evidence harness and the M10b
  failure/recovery matrix, including Windows shrink retry/recovery
  qualification and a default-off automatic-shrink control. M10a1/M10a3
  kernel tracing is optional diagnostic work; use M10aX only for a concrete
  diagnostic requirement unmet by host observation and bounded tracing.
  The M10b policy is selected: five-second observation, exact-target
  notifications after 30/60/120 seconds without progress, at most three
  notifications, an immutable 300-second deadline, a non-fatal latched stall,
  and separately qualified one-shot abandon-to-current recovery. Both shrink
  and re-notification remain default-off until hermetic and live gates pass.

## External blockers

- `guest-get-memory-stats` is absent from upstream QGA schemas; upgrading an
  upstream QGA is not a remedy. The custom adapter remains experimental and
  the host controller uses `dommemstat` by default.
- `dommemstat actual` is balloon provenance, not whole-guest or virtio-mem
  allocation. M9e now requires fresh, advancing `last-update`; live libvirt
  `current` remains allocation authority.
- Live resize remains gated by fresh XML validation and
  `requested == current` convergence at the time of each request.
- Automatic Windows shrink is directly observed both to make no progress for a
  one-block request and to make partial progress without retry for a 1 GiB
  request. It is unqualified, and Phase 2 supports only one active
  controller/device on this development host until M10b and M11.
- A hard QEMU/libvirt cgroup memory limit is recommended defense-in-depth for
  trusted `win11_gpu`; it is mandatory for future untrusted or production
  deployments.
- Live read-only inspection of signed `viomem.sys` `100.102.104.29400` found
  no supported user-mode state query. The upstream state record is available
  only through filterable kernel debug output. A signed, checksum-verified,
  bounded no-resize DbgViewCLI lifecycle capture loaded and unloaded its
  temporary driver without reboot, driver restart, boot logging, or debug-
  filter change, but observed no matching informational record. A later
  checksum-verified capture around the one-block operation also produced no
  matching record. This limits installed-driver diagnosis but does not block
  host allocation accounting.
- M10a2's shared-core evidence contract is implemented and hermetically tested:
  required host, Windows-health, controller, identity, explicit-unit, ordering,
  and converged-endpoint evidence fails closed when incomplete or mixed;
  driver trace remains optional.
- Persistent `win11_gpu` XML now retains a 1 GiB request and a five-second
  virtio-balloon statistics period. A graceful domain shutdown/start restored
  live `requested=current=1 GiB`; `viomem`, fresh telemetry, and the active
  controller were verified afterward with `NRestarts=0`.

The previous live convergence incident was resolved on 2026-08-18 at zero.
That evidence is historical: M9b subsequently bootstrapped and converged the
same device at 1 GiB. These observations prove convergence, while the
requested/plugged field semantics come from the pinned protocol/source chain.

## References

- [BACKLOG.md](BACKLOG.md) — execution source of truth
- [docs/roadmap.md](docs/roadmap.md) — milestone and phase status
- [docs/architecture.md](docs/architecture.md)
- [docs/feature-matrix.md](docs/feature-matrix.md)
- [docs/testing.md](docs/testing.md)
- [docs/upstream-virtio-mem-audit.md](docs/upstream-virtio-mem-audit.md)

Status is maintained alongside `BACKLOG.md` after each implementation session.
<!-- End of status document. -->
