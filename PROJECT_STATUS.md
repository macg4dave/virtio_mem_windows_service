# Project Status & Next Steps

**Updated:** 2026-09-09
**Phase:** Phase 2 — Core Functionality
**Overall status:** Windows and host service lifecycles plus single-VM host
actuation are live validated. The host-side demand join is complete with
native Windows and hermetic host evidence. M10b's bounded recovery scope is
closed from its hermetic matrix, installed rejection/no-replay result, and
bounded live shrink/recovery evidence. Installed ACL verification remains.
Host-stat freshness and the complete compatibility attestation are implemented
and installed; the corrected allocation-neutral hash still requires deliberate
attestation regeneration before the candidate controller can actuate, but that
operational refresh is no longer an M10b exit gate.
The allocation-authority contract is established from Virtio and pinned
implementation sources; optional driver tracing remains diagnostic.
`win11_gpu` is a fully trusted development/test KVM
guest; upstream Windows virtio-mem support remains technology preview.
Live shrink evidence has exposed the next design boundary: directional steps
and fixed no-progress deadlines cannot calculate durable guest demand.
M10e-M10g now define a quantitative absolute-target model, three-value
desired/requested/current reconciliation with separate control health, and
single-VM qualification before global arbitration.

## Completed locally

- M10e absolute desired-allocation estimation: checked physical and commit
  candidates, base/effective-maximum validation, normal and floor reserves,
  immediate growth, bounded warmed history, downward hysteresis, explicit
  capacity limitation, and atomic fingerprint-bound checkpoint recovery.
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
- Default-on automatic shrink, default-off re-notification, a fake-clock-tested
  30/60/120-second retry state machine, non-fatal latched stalls, and bounded
  `qualify-shrink`/`abandon-shrink` operator paths.
- Typed pre-command rejection versus unknown-command outcomes, operation-
  correlated shrink events, interruption/cancellation latching, rate-limited
  unowned-divergence observation, and deterministic restart/no-replay tests.

## Current evidence

The latest native RHEL gate passed:

- 55 shared-core and 60 host tests, with no failures.
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
  reclaim quantum per converged decision. Automatic reclaim is now enabled by
  default as a core product capability; the zero-progress outcome still
  latches further actuation and remains explicit platform-qualification evidence.

The 2026-09-09 M10b follow-up found the live device converged at 2,120 MiB and
the installed controller active after 31 systemd restarts. The old binary was
repeatedly rejecting legitimate allocation change as `domain_xml` drift
because libvirt's derived live `currentMemory` was still fingerprinted; it
later restarted on non-advancing `dommemstat`. The candidate fixes the false
drift input and recovery classifications locally. Its task-scoped service
rollout timed out at sudo authentication before execution and made no change.
The subsequent operator invocation failed a faulty XML-extraction precondition
before intended mutation. Prematurely armed rollback nevertheless performed a
clean stop/start, retained the prior installed hash and live allocation, and
returned the unit active with `NRestarts=0`. Extraction and rollback ordering
are corrected in the local batch.
The corrected batch then installed the candidate with matching SHA-256
`a1c431e67b49ba0373091fb32760cecea38a7e311bdc04a8972a9a44bf2a607c`
and the expected SELinux context. The unit stayed active with `NRestarts=0`,
emitted one typed pre-command attestation rejection without replay, and left
`requested=current=2105344 KiB` unchanged.

The subsequent active-controller reboot run was not valid qualification
evidence. QGA initiated the expected planned reboot at 13:43:55, but Windows
Installer initiated a second planned reboot at 13:50:31 to finish configuring
Sunshine. QEMU PID 379778 remained alive from September 7, ruling out a KVM
process crash; the latest Kernel-Power 41/6008 events predate this run and the
latest BugCheck 1001 is from August 18. Windows recovered with QGA, SSH, and
`qemu-ga` healthy, and the device remained converged at 2155872256 bytes. The
controller restarted seven times while reboot-time `dommemstat` omitted
`unused`, then returned active.
The privileged lifecycle prompt now requires hypervisor continuity,
independent Windows/service probes, pending-reboot and installer checks,
crash/reboot-event correlation, exactly one expected boot transition, and a
quiet stabilization window before attestation or other persistent work.

M10b/TASK-022 was closed by operator direction on 2026-09-09. The completed
hermetic interruption/restart/cancellation/ownership and typed-failure matrix,
installed candidate rejection/no-replay, bounded one-block retry and
abandon-to-current recovery, and larger partial/no-progress shrink probes are
accepted as sufficient milestone evidence. The clean active-controller reboot
and refreshed-attestation batch remains optional and is not claimed as passed.

## Open implementation work

- Implement M10f's distinct `desired`/`requested`/`current` reconciler. It must
  allow validated upward cancellation of a pending shrink when pressure
  returns, while forbidding another lower target and continuing to account
  from live `current`. Add write-before-command intent, fresh result
  resolution, stale-telemetry freeze, and a durable ambiguity/stall latch.
- Complete M10g controller and platform-reclaim qualification, including +4 GiB growth that
  later settles at +2 GiB demand, zero/partial shrink, renewed pressure,
  stale/replayed telemetry, ambiguous commands, cancellation, and restart.
  Automatic reclaim remains default-on; this gate determines qualification
  status and must expose constrained or latched operation clearly.

- Live-install the M10d Windows candidate and verify its protected ProgramData
  ACL. Atomic handoff/retention and durable restart-safe replay state are
  complete in code and tests; the current guest ProgramData directory is absent.
- Regenerate and review the installed M9d attestation after deploying the
  allocation-normalization fix. The implemented version-1 SHA-256 guard
  binds backend, memory-slot, VFIO, incompatible-workload, balloon, topology,
  trust, driver, QEMU, and libvirt evidence and rejects drift before resize.
- Provision ProgramData/configuration ACLs and package a classic Event Log
  message resource; SCM lifecycle/recovery and raw XML EventData are verified.
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
  Automatic reclaim now defaults on but latches after ambiguity or a bounded
  stalled operation. M10b's fixed retry/deadline profile remains
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
  No-Go because closed M10b evidence did not justify the interface and external
  driver ownership remains absent.
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
