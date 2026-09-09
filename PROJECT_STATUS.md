# Project Status & Next Steps

**Updated:** 2026-09-09
**Phase:** Phase 2 — Core Functionality
**Overall status:** Windows and host service lifecycles plus single-VM host
actuation are live validated. The host-side demand join is complete with
native Windows and hermetic host evidence. Bounded delivery and M10b recovery
logic are implemented and the one-block live recovery gate passed; installed
ACL verification and the wider interruption matrix remain. Host-stat freshness and the complete compatibility
attestation is implemented and awaits a separately approved live installation.
The allocation-authority contract is established from Virtio and pinned
implementation sources; optional driver tracing remains diagnostic.
`win11_gpu` is a fully trusted development/test KVM
guest; upstream Windows virtio-mem support remains technology preview.
Live shrink evidence has exposed the next design boundary: directional steps
and fixed no-progress deadlines cannot calculate durable guest demand.
M10e-M10g now define a quantitative absolute-target model, three-state
reconciliation, and single-VM qualification before global arbitration.

## Completed locally

- Architecture, API contracts, data model, engineering standards, testing
  strategy, roadmap, backlog, and QEMU Guest Agent setup documentation.
- Shared byte-based memory policy with alignment, bounds, hysteresis,
  requested/current convergence protection, 1 GiB growth quanta, and 64 MiB
  reclaim quanta.
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
- Production Windows raw telemetry schema version 2 with VM/service/session
  identity, dual-clock ordering, sequence, and explicit provenance, plus host
  freshness/replay/partial/size validation and target calculation joined to
  fresh alias-scoped live libvirt `current`.
- Atomic current-record publication with three-file retention, durable
  restart-safe host acknowledgement, and LocalService ProgramData ACL
  provisioning.
- Default-off automatic shrink/re-notification, a fake-clock-tested
  30/60/120-second retry state machine, non-fatal latched stalls, and bounded
  `qualify-shrink`/`abandon-shrink` operator paths.

## Current evidence

The latest native RHEL gate passed:

- 46 shared-core and 50 host tests, with no failures.
- `cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked`
- `cargo build -p virtio-mem-core -p virtio-mem-host --all-features --release --locked`
- `cargo fmt --all -- --check`
- `cargo clippy -p virtio-mem-core -p virtio-mem-host --all-targets --all-features --locked -- -D warnings`
- `bash -n scripts/*.sh`

The latest M10d Windows-native gate passes 67 tests with warnings denied and
produced a checksum-verified executable with SHA-256
`d91e6ccd2a0fdbac1da8bcd96a4e05ecf77964834e20d973c844e44d77d1e9dd`.

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

On 2026-09-08, the bounded one-block M10b qualification completed its three
30/60/120-second re-notifications, latched at 300 seconds, and successfully
recovered with abandon-to-current. A later paced 256 MiB ramp grew 1 GiB to
5 GiB, reclaimed 3,000 MiB, and then stalled at a 2,120 MiB current allocation.
That request was recovered to convergence. A subsequent automatic 64 MiB
request made no progress for 300 seconds and latched without restarting the
service. The service policy calculates one 1 GiB growth quantum or one 64 MiB
reclaim quantum per converged decision, but automatic reclaim remains disabled
because the live guest did not satisfy the "can shrink" condition.

## Open implementation work

- Implement M10e's quantitative absolute desired target using fresh Windows
  availability/commit counters, explicit fixed/base memory, configurable
  reserves, rolling history, hysteresis, safe floors, bounds, and alignment.
- Implement M10f's distinct `desired`/`requested`/`current` reconciler. It must
  allow validated upward cancellation of a pending shrink when pressure
  returns, while forbidding another lower target and continuing to account
  from live `current`.
- Complete M10g hermetic and live qualification, including +4 GiB growth that
  later settles at +2 GiB demand, zero/partial shrink, renewed pressure,
  stale/replayed telemetry, ambiguous commands, cancellation, and restart.
  Automatic reclaim remains disabled until this gate passes.

- Live-install the M10d Windows candidate and verify its protected ProgramData
  ACL. Atomic handoff/retention and durable restart-safe replay state are
  complete in code and tests; the current guest ProgramData directory is absent.
- Install a freshly reviewed M9d attestation with read-only service-account
  access before deploying this build. The implemented version-1 SHA-256 guard
  binds backend, memory-slot, VFIO, incompatible-workload, balloon, topology,
  trust, driver, QEMU, and libvirt evidence and rejects drift before resize.
- Provision ProgramData/configuration ACLs and package a classic Event Log
  message resource; SCM lifecycle/recovery and raw XML EventData are verified.
- Use the completed M10a2 correlated behavior-evidence harness to finish the
  remaining M10b interruption/restart matrix. The retry schedule, latched
  stall, and operator recovery are implemented hermetically and passed the
  one-block live qualification. The completed
  M10aX feasibility proposal addresses the diagnostic gap left by M10a1/M10a3
  without authorizing driver implementation or installation.
  The M10b policy is selected: five-second observation, exact-target
  notifications after 30/60/120 seconds without progress, at most three
  notifications, an immutable 300-second deadline, a non-fatal latched stall,
  and separately qualified one-shot abandon-to-current recovery. Automatic
  shrink remains default-off generally; the trusted development instance uses
  the selected 64 MiB quantum after deployment. Re-notification remains a
  separate control.

## External blockers

- `guest-get-memory-stats` is absent from upstream QGA schemas; upgrading an
  upstream QGA is not a remedy. The custom adapter remains experimental and
  the host controller uses `dommemstat` by default.
- `dommemstat actual` is balloon provenance, not whole-guest or virtio-mem
  allocation. M9e now requires fresh, advancing `last-update`; live libvirt
  `current` remains allocation authority.
- Live resize remains gated by fresh XML validation and
  `requested == current` convergence at the time of each request.
- Automatic Windows shrink is size-sensitive: one block and 64 MiB made no
  progress, while the paced 256 MiB ramp reclaimed 3,000 MiB before stalling.
  Automatic reclaim is disabled. M10b's fixed retry/deadline profile remains
  diagnostic/recovery behavior and does not determine desired memory. Phase 2
  still supports only one active controller/device on this host until M11.
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
  host allocation accounting. M10aX now specifies a versioned cached read-only
  status IOCTL, administrator/SYSTEM access, external build/signing,
  disposable-guest testing, and exact rollback gates; implementation remains
  No-Go pending M10b operational value and external driver ownership.
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
