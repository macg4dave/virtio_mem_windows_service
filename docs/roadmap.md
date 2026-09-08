# Roadmap

## Status legend

- `[ ]` Not started
- `[~]` In progress or partially complete
- `[x]` Complete with local evidence and documentation
- `[!]` Blocked by an external dependency

## Current position

The project is in **Phase 2: Core Functionality**. The parser, resize policy,
QEMU Guest Agent client boundary, wakeable polling loop, portable service host,
basic configuration model, startup validation path, and live Windows SCM
lifecycle/recovery path are implemented and tested. The
workspace contains an installed, active single-VM host controller with
XML/state validation and a bounded runtime loop. The next unprivileged
implementation priority is M10d report session/sequence/provenance identity
and bounded delivery after the completed Phase 2 host-side join. M9d binds the expanded
compatibility attestation, and M9e now freshness-qualifies host telemetry. The
existing one-VM host controller
remains the only resize authority until Phase 3 global arbitration has been
validated. The Virtio specification and pinned QEMU/libvirt/virtio-win
sources establish `requested`/`current` as the host allocation contract;
optional driver tracing qualifies installed-binary behavior rather than
gating accounting. M8 live QGA/KVM validation is complete on `win11_gpu`, including
isolated agent restart and graceful guest reboot recovery.
M10a read-only discovery found no supported installed-driver query for
`requested_size`/`plugged_size`. That limits guest-side diagnosis but no
longer blocks M10c/M10d or hermetic M11 simulation. Bounded kernel-debug
capture remains optional, separately approved evidence for notification,
branch, and shrink-recovery behavior. M10aX now records a proposal for the
unmet stalled-shrink diagnostic need, but keeps implementation No-Go pending
M10b operational value, external driver ownership, security/signing review,
and disposable-guest rollback.
The QGA memory command may be unavailable on the guest, but that is no longer
a Windows service startup blocker because the service uses native
`GlobalMemoryStatusEx` and `GetPerformanceInfo` telemetry. The host controller
uses `dommemstat` by default. The upstream audit found that
`guest-get-memory-stats` is not an upstream QGA command; M9e corrected the
`dommemstat` mapping and added freshness/advancement gates. `win11_gpu`
is a fully trusted development/test guest; Windows virtio-mem remains
technology preview.

## Verified evidence

- **Current platform gates:** the latest RHEL gate passes 29 shared-core and
    35 host tests; the latest native-Windows gate passes 64 tests. Keep these
    as separate supported-platform results rather than one workspace total.
- **Safe policy core:** resize decisions are aligned, bounded by configured
    limits, hysteresis-aware, and blocked while `requested != current`.
- **Strong boundary separation:** guest Rust code does not invoke Linux commands;
    host Bash validation stays explicit-scope and read-only by default.
- **Failure visibility:** parser, transport, startup, polling, resize, and
    worker failures return typed errors instead of silent fallback.
- **Cancellation correctness:** stop wakes the polling wait rather than
    delaying shutdown for the full interval.
- **Active host controller:** the RHEL host adapter validates live XML,
    selected alias data, QGA responses, and resize requests before sending a
    change, and the runtime loop blocks overlapping updates until convergence.
- **Documentation traceability:** architecture, API, testing, backlog, and
    roadmap status are updated together with implementation work.
- **Host-side virtio-mem reality check:** libvirt/QEMU guidance confirms a
    virtio-mem resize is an asynchronous `requested` change, not an immediate
    guest memory state switch; the controller must wait for convergence before
    issuing a follow-up request.
- **Historical host fallback implementation (2026-08-18; superseded by M9b live evidence):** the
    host controller can now source memory stats from `virsh dommemstat`
    instead of the unimplemented `guest-get-memory-stats`, and a shared
    `MIN_HEADROOM_BYTES` invariant plus a host `/proc/meminfo` headroom gate
    are enforced in code rather than only by configuration.
- **RHEL read-only fallback evidence (2026-08-18):** `virsh dommemstat
    win11_gpu` returned `actual`, `unused`, and `available` successfully, so
    the default host stats source is observable on this guest. Live XML remains
    the authoritative source for the requested/current convergence gate.
- **Historical RHEL service observation (2026-08-18; superseded by M9b):** the read-only check found no
    installed `virtio-mem-host@win11_gpu.service` and no journal entries.
    Service installation and lifecycle validation remain separate approved
    mutation work.
- **M9b installed-controller completion (2026-09-05):** an approved privileged
    batch replaced a stale pre-zero-state binary, installed the fixed
    `win11_gpu` configuration, and started the existing enabled systemd
    instance. The controller issued one guarded bootstrap from zero to the
    aligned 1 GiB minimum, reached `requested=current=1073741824`, retained
    that minimum after another poll, and remains active with no restart.
- **M8 live QGA/KVM completion (2026-09-04):** approved password-once
    batch confirmed QGA `110.0.2`, Windows 11 x64 / `ICE101`, three successful
    `guest-info` calls at 77–124 ms, and three valid `dommemstat` fallback
    samples at 84–129 ms. Live XML shows the QGA channel connected and
    `ua-virtiomem0` converged at `requested=current=0`. An isolated `qemu-ga`
    restart recovered, a graceful QGA-mode reboot completed, and the full
    probe passed again afterward at 79–124 ms for `guest-info` and 100–112 ms
    for `dommemstat`. QGA does not implement the nonstandard
    `guest-get-memory-stats` extension. M9e subsequently freshness-qualified
    the fallback; live libvirt `current` remains authoritative for virtio-mem
    allocation.
- **M9 live Rust XML-adapter completion (2026-09-05):** the authoritative
    Rust CLI selected `ua-virtiomem0`, reported the live 20 GiB size, 2 MiB
    block, and `requested=current=0` state in canonical bytes, rejected a
    nonexistent alias, and failed closed on unknown M9a compatibility evidence.
    Before/after live XML SHA-256 values matched. The shared contract now
    accepts zero observed state while continuing to reject zero targets.
- **M9a live compatibility completion (2026-09-05):** a bounded Rust QMP
    source confirmed `dynamic-memslots=true` and
    `unplugged-inaccessible=on` on the selected live QOM device. The 2 MiB
    block matches host THP, native arguments show `mem-lock=off`, three VFIO
    devices were identified as GPU/audio/USB rather than NVMe, and the operator
    confirmed no RDMA or unsupported vhost-user workload dependency. The exact
    dry-run vector passed without `--apply` and live XML remained unchanged.
    This evidence applies to the exact trusted development configuration;
    M9d subsequently bound the broader upstream-attestation fingerprint.
- **M9d hermetic compatibility drift guard (2026-09-07):** a version-1
    SHA-256 attestation binds reviewed workload/trust/driver/slot/VFIO facts to
    allocation-neutral live domain XML and QEMU argv, alias-scoped QMP
    properties, and QEMU/libvirt versions. Every resize recollects those inputs
    and fails closed on missing, malformed, tampered, or drifted evidence;
    allocation convergence alone does not invalidate the review.
- **M9e host telemetry completion (2026-09-08):** `dommemstat actual` is
    retained only as balloon provenance; required `unused` and `available`
    counters may exceed it, while live XML `current` remains allocation
    authority. Injected-clock tests reject missing, stale, future, regressing,
    and non-advancing `last-update`. The Rust `decision` command now uses the
    controller's configured source, live XML, and exact policy evaluator, and
    the duplicate QGA-only Bash preview is removed.
- **Upstream audit (2026-09-05):** pinned review found incomplete
    `dommemstat` semantics/freshness, a non-upstream QGA memory command,
    additional backend/slot/VFIO/balloon/version compatibility inputs, an
    unqualified Windows shrink retry path, and unsafe independent multi-instance
    reservation. Findings are tracked by M9d, M9e, M10b, M10c, and M11 in
    [`upstream-virtio-mem-audit.md`](upstream-virtio-mem-audit.md).
- **Historical convergence recheck (2026-08-18; superseded by the 1 GiB M9b state):** `win11_gpu` reported
    `requested=0 KiB` and `current=0 KiB` after the latest Windows driver
    update. The previous rollback convergence blocker is resolved. The QGA
    responses checked here do not expose Windows driver
    `requested_size`/`plugged_size`; that historical observation did not itself
    establish field semantics. The later pinned protocol/source review does.
- **Historical Windows service handoff (2026-08-18; superseded by M7):** RHEL-side QGA checks
    confirm the guest is running and responds with hostname `ICE101`, but they
    cannot observe Windows SCM state directly. The native telemetry worker is
    therefore considered a Windows-side runtime claim until SCM/log evidence
    is supplied. QGA memory stats remain unavailable, while `dommemstat`
    remains usable; virtio-mem now reports `requested=0 KiB` and
    `current=0 KiB` after the driver update.
- **Windows demand-agent foundation (2026-08-18):** native
    `GlobalMemoryStatusEx`/`GetPerformanceInfo` collection, checked
    canonical-byte snapshots, bounded pressure ratios, provisional five-state
    classification, aligned bounded recommendations, and advisory safe floors
    are implemented behind deterministic tests. A one-cycle collection and
    injected publication boundary is also implemented; the report cannot
    directly actuate virtio-mem.
- **VS Code RHEL-to-Windows build workflow (2026-09-04):** added a
    non-mutating Bash/VS Code task path that synchronizes the working tree to a
    Windows KVM build guest, initializes MSVC, runs native Cargo validation,
    and verifies the fetched service executable by SHA-256. The guest-side
    OpenSSH, Rust MSVC, and Visual Studio Build Tools bootstrap remains an
    operator setup step; service deployment and live KVM changes remain
    separate approval gates.
- **Rust-only explicit host control (2026-09-04):** the Rust host CLI now owns
    alias-scoped snapshot/validation, exact dry-run argument reporting, and
    explicitly applied one-shot resize. It reuses compatibility, convergence,
    canonical-unit, device-headroom, and host-headroom gates; the duplicate
    Bash resize helper is removed.
- **Windows recovery observability foundation (2026-09-04):** the SCM path
    emits bounded Application Event Log lifecycle/failure records with stable
    IDs and treats a worker exit without cancellation as a non-zero failure.
    Live M7 validation completed a clean lifecycle and observed the first
    configured recovery restart 5.05 seconds after an invalid-config failure.
    Raw XML EventData is authoritative until message-resource packaging lands.

## RHEL-controlled build and test plan

### Goal and boundary

Use the RHEL development host as the single control plane while executing each
check on its supported platform:

1. RHEL builds, tests, and lints `virtio-mem-core` and `virtio-mem-host`, then
   validates Rust formatting and Bash syntax.
2. RHEL synchronizes Git-tracked and non-ignored working-tree files to one
   explicitly configured Windows SSH endpoint.
3. Windows initializes the native MSVC environment and performs the locked
   service release build, unit tests, rustfmt check, and warnings-as-errors
   Clippy check.
4. RHEL retrieves `virtio-mem-service.exe` and accepts it only when its local
   SHA-256 matches the checksum calculated on Windows.

The aggregate entry points are the VS Code **Build: all non-mutating gates**
task and `VIRTIO_MEM_WINDOWS_SSH=ALIAS make all-gates`. They do not install or
start either service, modify libvirt or systemd, change guest memory, or claim
live integration evidence.

### Delivery stages

- [x] **Stage A — Native RHEL gate:** `scripts/build-rust.sh` uses the workspace
  lockfile and validates the shared core and host controller without trying to
  compile Windows SCM APIs for Linux. Current evidence is a release build, 29
  shared-core tests, 35 host tests, rustfmt, warnings-as-errors Clippy, and
  Bash syntax.
- [x] **Stage B — RHEL orchestration:** `scripts/windows-remote-build.sh`,
  `.vscode/tasks.json`, and the Makefile provide explicit endpoint checking,
  one-sync native validation, verified artifact retrieval, and both editor and
  terminal entry points.
- [x] **Stage C — Windows endpoint bootstrap:** the current Windows build login
    now has key-based OpenSSH access, Rust MSVC plus rustfmt/Clippy, Visual
    Studio C++ Build Tools and Windows SDK, `tar.exe`, and `certutil.exe`. The
    endpoint is listening on its private KVM interface and its credentials
    remain operator-owned. Migration to a separate least-privilege build
    account remains outstanding after the cross-host path is proven.
- [x] **Stage D — First end-to-end native gate:** the aggregate gate completed
    against `ice101.lan` with a release build, 59 passing Windows tests, zero
    doctest failures, rustfmt, warnings-as-errors Clippy, and a verified
    artifact SHA-256 of
    `56572af65f9a9297a5ff755dd01e7a9908d93c037a25cf8419b3113d9ae4b16e`.
- [x] **Stage E — Repeatability evidence:** the fingerprint-pinned milestone
  runner completed two consecutive RHEL and Windows aggregate gates, retained
  both logs and artifact hashes, and produced the same verified executable
  checksum. Earlier toolchain, doctest, and checksum-parser failures propagated
  non-zero until corrected.
- [~] **Stage F — Live validation:** M7 SCM lifecycle/recovery, M8 QGA/reboot,
  and M9/M9a/M9b host inspection, compatibility, installation, bootstrap, and
  convergence evidence pass. Cross-layer capture, report integration, and the
  failure/recovery matrix remain under separate approval procedures.

### Blockers and exit criteria

| ID | Status | Blocker | Impact | Resolution evidence |
| --- | --- | --- | --- | --- |
| BUILD-001 | Resolved | The configured Windows SSH/MSVC endpoint had not been checked from the RHEL control plane | Cross-host reachability and authentication were unproven | `windows-remote-build.sh check` passes through the explicit `virtio-mem-windows` SSH alias |
| BUILD-002 | Resolved | The remote wrapper had not completed one end-to-end run | Command quoting, remote path handling, MSVC initialization, and checksum retrieval were unproven | `windows-remote-build.sh all` exits zero with 59 passing native tests and matching SHA-256 values |
| BUILD-003 | Resolved | Windows SCM validation required an elevated Windows session and service registration changes | Default build success could not establish install/start/stop/recovery behavior | M7 live lifecycle and 5-second recovery restart passed on `ice101.lan` with full rollback |
| BUILD-004 | External release gate | Live QGA, systemd, libvirt, and virtio-mem convergence checks require named targets and explicit mutation approval | Default build success cannot establish live runtime or resize readiness | Separately approved M8–M10b procedures pass with rollback and convergence evidence |

**Milestone exit:** BUILD-001 and BUILD-002 are closed, two consecutive
aggregate gates pass against the explicit Windows endpoint, the fetched
executable is checksum-verified, and exact native Windows plus RHEL results are
recorded. BUILD-003 is now closed by M7 evidence. BUILD-004 remains outside
this milestone and does not block ordinary developer builds.

## Verified wins to preserve

- **Local quality baseline:** the current platform gates pass 29 core, 35 host,
    and 64 Windows tests plus their release build and Clippy checks.
- **Safe policy core:** resize decisions are aligned, bounded by configured
    limits, hysteresis-aware, and blocked while `requested != current`.
- **Clear boundaries:** guest Rust code does not invoke Linux commands; host
    Bash validation remains explicit-scope and read-only by default.
- **Failure visibility:** parser, transport, startup, polling, resize, and
    worker failures have typed/error-return paths instead of silent fallback.
- **Cancellation correctness:** stop wakes the polling wait rather than
    delaying shutdown for the full interval.
- **Documentation traceability:** architecture, API, testing, backlog, and
    roadmap status are updated together with implementation work.
- **Host-side virtio-mem reality check:** libvirt/QEMU documentation confirms a
    virtio-mem resize is an asynchronous `requested` change, not an immediate
    guest memory state switch; the controller must wait for convergence before
    issuing a follow-up request.

## New findings from official QEMU/libvirt guidance

These are now design requirements rather than optional future refinements:

- `requested-size` must be an integer multiple of the device `block-size` and
    cannot exceed the device's maximum size.
- `block-size` is the hotplug granularity and should usually be at least the
    guest THP size; 2 MiB is the common default for x86 guests.
- `current` may lag behind `requested` while the guest plugs or unplugs blocks;
    this is normal and must not trigger a second resize request.
- QEMU does not provide a balloon-style protection layer for unplugged memory.
    A hard QEMU/libvirt cgroup memory limit is recommended defense-in-depth for
    fully trusted development guest `win11_gpu` and mandatory for future
    untrusted or production guests.
- `dynamic-memslots=on` is recommended when available and must be combined with
    `unplugged-inaccessible=on` for the virtualization stack to treat unplugged
    blocks as inaccessible.
- Some features and device types remain incompatible or require explicit
    qualification: vDPA, RDMA migration, VFIO-NVMe, `mlock`, encrypted/secure
    virtualization, active balloon resizing, memory-slot/VFIO mapping limits,
    and vhost-user backend/version combinations such as DPDK/SPDK.
- For virtio-mem memory backends, sparse semantics are expected: `reserve=off`
    and `prealloc=off` for the backend, while `prealloc=on` is sometimes used on
    the virtio-mem device itself.
- Libvirt `dommemstat actual` is the balloon value, not whole-guest or
    virtio-mem allocation. `available > actual` is not inherently invalid, and
    `last-update` must be freshness-checked before policy use.
- Upstream supports multiple virtio-mem devices, but independent Phase 2
    controller instances cannot atomically reserve one host pool. Only one
    controller/device is supported on the development host until M11.
- The reviewed Windows driver does not expose an obvious periodic retry timer
    for a no-progress shrink. Automatic shrink requires M10b qualification and
    a default-off control before automated reclaim is supported.

These findings should be treated as the baseline for live validation and release
readiness in the remaining host-side work.

## Milestone map

| ID | Milestone | Status | Depends on | Exit evidence |
| --- | --- | --- | --- | --- |
| M0 | Repository and architecture baseline | [x] | — | Architecture, contracts, standards, and testing docs reviewed |
| M0a | RHEL-controlled cross-platform developer gate | [x] | M0 | RHEL core/host gate and two fingerprint-pinned aggregate native runs pass; both produce the same checksum-verified Windows executable |
| M1 | Pure memory policy and QGA parsing | [x] | M0 | Parser and controller tests cover malformed, boundary, alignment, and convergence cases |
| M2 | Guest runtime polling foundation | [x] | M1 | Poller, named-pipe client boundary, wakeable scheduler, bounded operation deadlines, and transport/error tests pass |
| M3 | Service lifecycle foundation | [x] | M2 | Startup readiness, cancellation, failure, state, bounded shutdown, and live SCM observation pass |
| M4 | Runtime configuration foundation | [x] | M2 | Versioned JSON schema, persistent loading, identity, endpoint, report path, timing, account, missing-file defaults, and basic validation exist; stronger production bounds, ACLs, and atomic update remain H3 work |
| M5 | Native Windows SCM adapter | [x] | M3, M4 | Elevated Program Files lifecycle passed under LocalService with stable live Event Log records, bounded callbacks, clean-stop exit zero, and failure exit one |
| M6 | Concrete guest runtime wiring | [x] | M4, M5 | Interactive and SCM paths publish VM-scoped raw native Windows telemetry without opening QGA, accepting host allocation, or exposing a guest resize sink; native Windows tests pass |
| M7 | Installation and recovery operations | [x] | M5, M6 | Live install/start/observe/stop/delete passed; 5-second recovery restart and 5/30/60 metadata were verified, rollback restored the original running service |
| M8 | Live QGA and KVM validation | [x] | M2 | Repeated QGA and `dommemstat` probes, connected-channel XML, isolated QGA restart recovery, graceful guest reboot recovery, and unchanged convergence all passed on `win11_gpu` |
| M9 | Host virtio-mem XML adapter | [x] | M1, M8 | Live Rust CLI snapshot/validation, exact alias selection, canonical zero-state parsing, wrong-alias rejection, fail-closed dry run, and before/after non-mutation evidence pass on `win11_gpu` |
| M9a | Virtio-mem safety and compatibility gate | [x] | M8, M9 | Fresh live QMP properties, THP/block match, explicit operator review, VFIO device classification, locked/RDMA/vhost-user exclusion, exact dry run, and XML non-mutation passed on `win11_gpu` |
| M9b | RHEL systemd host controller | [x] | M1, M8, M9, M9a | Installed one-VM systemd controller completed a guarded zero-to-1-GiB bootstrap on `win11_gpu`, converged, retained its minimum, and remains active without overlapping requests |
| M9c | Rust host CLI replaces Bash resize helper | [x] | M9, M9a | Rust owns snapshot, validation, exact dry-run arguments, and explicitly applied resize commands; hermetic regression tests pass and the duplicate Bash helper is removed |
| M9d | Compatibility attestation drift guard | [x] | M9a, M9b | Version-1 reviewed evidence is SHA-256-bound to allocation-neutral live domain/QEMU/QMP/version inputs; hermetic exact-match, tamper, and drift tests pass |
| M9e | Host telemetry correctness and freshness | [x] | M8, M9b | Balloon semantics, bounded advancing `last-update`, live-XML allocation authority, and the shared-path Rust decision preview pass hermetic tests |
| M10 | Phase 2 demand-agent foundation | [~] | M4, M6 | Native telemetry, raw production publication, shared calculator, bounded pressure state, desired target, advisory safe floor, and host-side allocation join are tested; bounded delivery and workload evidence remain |
| M10c | Host-side current-allocation join | [x] | M9e, M10 | A fresh VM-scoped raw Windows envelope is joined with alias-scoped live libvirt `current`; 31 core, 40 host, and 66 native Windows tests pass without guest allocation input or resize authority |
| M10d | Demand envelope and bounded delivery | [~] | M10c | Version 2 identity/provenance, replay checks, and malformed/partial/oversized read rejection are implemented; ACLs, durable handoff, retention/rotation, and restart-safe replay remain |
| M10a | Allocation-authority contract | [x] | M8, M9, M9a | Virtio 1.2 plus pinned QEMU/libvirt/virtio-win sources define `requested`/`current` semantics; live alias-scoped libvirt `current` is authoritative and driver debug output is diagnostic, not an accounting dependency |
| M10a1 | Optional driver diagnostic qualification | [x] | M8, M9a | Signed DbgViewCLI completed a bounded no-resize kernel capture without boot/debug-filter/viomem changes; no matching informational record appeared and exact later cleanup removed all temporary process/service/file/registry state |
| M10a2 | Correlated behavior-evidence harness | [x] | M8, M9a | Versioned shared-core JSON validation requires ordered clocks, repeated operation/VM/device identity, explicit bytes, stable/converged libvirt endpoints, Windows health, and controller state; driver records are optional diagnostics |
| M10a3 | Optional bounded driver observation | [x] | M9b, M10a2 | One 2 MiB grow converged, but the predeclared 1 GiB recovery target remained 2 MiB above current for 60 samples/300 seconds; no matching driver record appeared, no overlapping request was issued, and graceful domain recreation restored convergence/controller state |
| M10a4 | State-contract adoption | [x] | M10a | Architecture, API, data model, and testing docs make live libvirt `current` authoritative while distinguishing requested, converging, stalled, and Windows diagnostic evidence |
| M10aX | Conditional driver status-interface feasibility | [x] | Concrete unmet diagnostic need | The M10a3 no-progress and larger partial-progress stalls plus empty bounded captures justify a proposal for a cached read-only status IOCTL; security, ABI, tests, external build/signing/install, and rollback gates are specified, while implementation remains No-Go |
| M10b | Single-VM failure, Windows shrink, and recovery matrix | [~] | M7, M9b, M9e, M10a2 | Live probes prove no-progress and partial-progress-without-retry plus graceful domain-recreation recovery; the selected default-off 30/60/120-second, three-notification, 300-second policy and abandon-to-current path now require hermetic and live qualification with the remaining failure matrix |
| M11 | Phase 3 global pool simulation | [ ] | M9e, M10d, M10a | Hermetic multi-VM simulation models atomic host reserve, actual allocations, pool-free capacity, growth/reclaim priorities, stale reports, and all five pressure states; live multi-target actuation additionally requires M9d and M10b |
| M11a | Controlled reclaim and convergence | [ ] | M11 | Trend-aware safe floors, bounded aligned reclaim, hysteresis, in-flight protection, convergence waits, and stop-on-pressure behavior pass simulation tests |
| M12 | Hardening and observability | [ ] | M11a | Recovery, event logging, metrics, bounded timeout behavior, and restart tests pass for guest and global-controller paths |
| M13 | Operational release readiness | [ ] | M12 | Documentation, health checks, monitoring, compatibility evidence, rollback, and repeatable host automation complete |

### M10b selected bounded Windows-shrink policy

M10b will qualify one explicit host-side state machine. These values are the
initial qualification profile, not workload-tuned production defaults:

1. Automatic Windows shrink and same-target re-notification are separate
   controls and both default to disabled. A shrink can start only from fresh,
   compatible, converged state with no other in-flight operation. Its immutable
   target is block aligned, does not cross the current safe floor, and is
   bounded by the configured reclaim quantum.
2. After the initial request, sample the alias-scoped live `requested` and
   `current` fields every five seconds. A reduction of `current` by at least one
   device block is progress. Progress moves the no-progress timestamp but never
   replenishes the retry budget or extends the hard deadline.
3. If `requested` still equals the immutable target and `current` remains above
   it, re-notify that exact target after no-progress delays of 30, 60, and 120
   seconds. Allow at most three re-notifications and one operation deadline of
   300 seconds from the initial request. Skip a retry whose observation window
   would exceed the hard deadline. Never derive a new shrink target while the
   operation is pending.
4. Re-notification is a dedicated recovery operation, not an ordinary resize:
   it is permitted only for `requested == target < current`, after a fresh
   alias, compatibility, health, alignment, and operation-ownership check.
   Any external change to `requested`, invalid/non-monotonic state, stale input,
   cancellation, guest transition, or command ambiguity stops notification and
   latches the operation for recovery.
5. Convergence (`requested == current == target`) completes the operation. A
   300-second deadline or exhausted safe path produces a latched
   `ShrinkStalled` state; it is not a fatal worker error, must not cause a
   systemd restart loop, and suppresses further automatic actuation for that
   VM/device while observation and health reporting continue.
6. The preferred non-disruptive recovery is an explicitly qualified
   **abandon-to-current** operation: after two unchanged fresh samples, re-read
   immediately before applying and raise `requested` to that aligned observed
   `current`. This retains any blocks already reclaimed. It receives one
   30-second convergence window and no retry. Until M10b proves this path live,
   it remains operator-approved only. Failure leaves actuation latched off;
   graceful domain recreation from a known persistent definition is the final
   operator-approved fallback, never an automatic forced destroy.
7. Cancellation and process restart never replay a resize or re-notification.
   A restarted controller that observes divergence without an active operation
   it can prove it owns enters recovery-required observation. An uncertain
   command result is reconciled by a fresh read: observe if the target is
   visible, retry transport only if non-application is proven, otherwise latch.
8. Emit one structured event for request, block progress, each re-notification,
   convergence, ownership conflict, stall, cancellation, and recovery outcome.
   Per-poll unchanged-state messages are rate limited. Record operation ID, VM,
   alias, immutable target, requested/current bytes, block counts, retry index,
   progress time, deadline, and reason without logging guest data.

The timings are evidence-led: the 1 GiB probe made all observed progress in
the first sample and then stayed unchanged, while both live stalls were still
divergent at 300 seconds. A 30-second first wait gives the initial driver pass
ample room; increasing intervals avoid notification pressure; and the hard
deadline preserves the already exercised recovery bound. More frequent or
longer retry is not justified by the present evidence.

Hermetic tests must exercise the full state table with a fake clock before the
two live qualifications: same-target re-notification must cause additional
progress or convergence, and abandon-to-current must safely converge after
both zero-progress and partial-progress stalls. If either live qualification
fails, automatic Windows shrink stays disabled and M10b documents the
operator-only recovery path instead of increasing the retry budget.

## Phase 1 — Foundation

### F1. Repository baseline — complete

- [x] Rust 2021/MSVC project structure established.
- [x] Rust-only runtime and Bash-only automation boundaries documented.
- [x] API, data model, architecture, engineering, and testing documents created.
- [x] Backlog and validation scripts established.

**Gate:** Documentation and repository rules are present before runtime changes.

### F2. Custom guest-agent adapter contract — complete as an experimental boundary

- [x] Upstream `guest-info` use and the custom/downstream
    `guest-get-memory-stats` request/response adapter are distinguished.
- [x] Required and optional memory fields defined.
- [x] Error behavior for malformed and inconsistent responses defined.

**Gate:** Parser behavior is testable without a live VM.

### F3. Local validation baseline — complete

- [x] Release build, unit tests, formatting, and Clippy commands documented.
- [x] Host prerequisite and read-only QGA probe scripts added.

**Gate:** Local Rust validation passes; live checks remain explicitly separate.

## Phase 2 — Core Functionality

### F4. Memory policy and polling — complete

- [x] Hysteresis policy grows/shrinks by aligned blocks.
- [x] Minimum, maximum, and convergence limits are enforced.
- [x] `MemoryPoller` composes QGA responses with the controller policy.
- [x] `run_polling_loop` validates intervals and stops on cancellation or failure.
- [x] Cancellation wakes the scheduler instead of waiting for the full interval.
- [x] Enforce a configured deadline around connect, write, and read for the
    QGA request boundary; the Windows transport uses overlapped I/O and
    `CancelIoEx` rather than an unbounded synchronous flush.
- [x] Prevent a slow or stuck QGA operation from holding the polling caller
    past the shutdown deadline; timeout cancellation closes the overlapped
    request before returning the transport error.

**Gate:** Pure Rust tests pass and no host command is invoked by guest logic.

### F5. Guest transport boundary — complete

- [x] Configurable Windows named-pipe client sends newline-delimited QGA JSON.
- [x] Transport, empty-response, parser, and policy errors remain explicit.
- [x] Confirm newline framing, response correlation, and malformed envelope
    handling against deterministic captured-response fixtures; the Windows
    transport uses a stable request id and requires one matching response
    frame.
- [x] Validate the host-facing QGA channel, service lifecycle, permissions,
    and response format on the Windows KVM guest; the Windows service uses
    native telemetry and does not contend for the QGA-owned pipe.

**Gate:** Three consecutive read-only QGA probes succeed on the real VM.

### F6. Service lifecycle and basic configuration — complete

- [x] `ServiceHost` models startup readiness, running, stopping, stopped, and failed states.
- [x] Startup failures are distinct from runtime worker failures.
- [x] Stop and shutdown share one wakeable cancellation path.
- [x] Service identity, legacy adapter endpoint, poll interval, shutdown timeout, and least-privilege account defaults receive basic validation.
- [x] Load persistent configuration rather than relying only on in-process defaults.
- [x] Add a versioned configuration schema and migration/rejection rules.
- [x] Enforce the configured shutdown timeout during worker termination and
    return a typed failure when a worker does not stop before the deadline.

**Gate:** Worker readiness precedes `Running`; expected cancellation is not a crash; unexpected failures remain recoverable by the SCM layer.

### F6a. Contract and unit safety — must-have before live resize

- [x] Choose bytes (`u64`) as the canonical internal memory unit and document
    every conversion boundary.
- [x] Reconcile QGA bytes, controller bytes, libvirt XML values, and any
    `virsh` command units before enabling a resize sink; the pure Rust
    contract, captured XML parser, host XML source, and resize sink now use
    checked canonical-byte/KiB boundaries. Live discovery remains separate.
- [x] Reject zero size, undersized/non-power-of-two block size, zero resize
    targets, out-of-range values, and unaligned values in the pure Rust
    contract and host XML adapter; observed requested/current may be zero for
    a fully unplugged device.
- [x] Enforce `requested % block == 0`, `requested <= size`, device-size
    alignment, and `block >= 1 MiB` checks before issuing a resize request.
- [x] Add boundary tests for maximum values and unit conversion round trips.
- [x] Add compatibility checks for `dynamic-memslots`/`unplugged-inaccessible`
    and known incompatible device classes before enabling live automation;
    initial live M9a evidence passed. M9d now fingerprints backend,
    slot/mapping, balloon, incompatible-workload, topology, trust, driver, and
    stack-version evidence and revokes authorization after configuration drift.
- [x] Complete M9e `dommemstat` semantics and freshness validation; balloon
    `actual` must not bound `unused`/`available` or substitute for live
    virtio-mem `current`.

**Gate:** A target size can be traced from its memory observation to host request with
no ambiguous or implicit unit conversion.

### F7. Native Windows service integration — complete

- [x] Implement the Rust SCM dispatcher and service callback adapter.
- [x] Report `SERVICE_START_PENDING` with bounded wait hints/checkpoints.
- [x] Report `SERVICE_RUNNING` only after worker initialization succeeds.
- [x] Accept stop and system-shutdown controls and signal `StopSignal`.
- [x] Report `SERVICE_STOP_PENDING` during bounded shutdown, then `SERVICE_STOPPED`.
- [x] Return a non-zero process result for unexpected worker failure.
- [x] Add a local Windows service registration and stop path that uses the SCM
    APIs exposed by the currently installed `winapi` crate.
- [x] Validate the install/start/stop lifecycle on a real Windows guest with
    service manager permissions; the elevated Program Files run and raw Event
    Log observation passed under `LocalService`.

**Gate:** SCM lifecycle tests pass on Windows and callbacks remain bounded/non-blocking.

### F8. Concrete runtime wiring

- [x] Replace the `main.rs` foundation stub with service/interactive-mode dispatch.
- [x] Connect interactive and SCM workers to native Windows memory telemetry;
    they do not open the QGA virtio-serial device owned by `QEMU-GA`.
- [x] Implement guest-side raw demand acquisition; native telemetry is
    collected, validated, VM-scoped, timestamped, and published without
    attempting to establish virtio-mem `current` allocation.
- [x] Wire raw telemetry publication for the selected M10c host-side join with
    alias-scoped live libvirt `current`. Host policy calculates the target; do
    not add a guest resize sink or allocation feed.
- [x] Add structured error context at the service boundary for configuration,
    worker construction/initialization, and service-host execution failures.
- [x] Add deterministic fake state-provider and resize-sink boundaries for
    legacy integration tests. They are test seams, not intended Windows
    production actuation.

**Gate:** The executable can start its worker, stop cleanly, and fail visibly when an adapter fails.

### F8a. Failure-injection and contract harness

- [~] Simulate QGA timeout, disconnect, malformed JSON, partial response, and
    stale data without a live VM.
- [~] Simulate resize rejection, non-convergence, guest reboot, and service
    restart through fakes.
- [~] Verify no resize is issued after cancellation or while a request is
    pending.
- [ ] Keep automatic Windows shrink disabled by default and prove bounded
    retry and controlled recovery under the selected M10b state machine.
- [ ] Prove the 30/60/120-second same-target schedule, three-notification
    budget, immutable 300-second deadline, progress handling, and latched stall.
- [ ] Prove cancellation/restart never replay work and a stall does not turn
    into a service-manager restart loop.
- [x] Keep the harness independent of Linux tools and production VM state.

**Gate:** Every failure mode in the service boundary has deterministic local
evidence before live actuation expands beyond the validated M9b bootstrap.

### F9. Installation and recovery

- [x] Define stable service name, display name, description, executable path,
    startup mode, account, and bounded failure-action delays in the SCM
    registration.
- [x] Install with the configured `LocalService` account; the current native
    telemetry worker does not open the QGA-owned channel.
- [x] Configure bounded restart delays only for unexpected/non-crash failures;
    live failure exit and the first 5-second recovery restart are verified.
- [x] Emit stable, bounded Windows Application Event Log records for SCM
    lifecycle transitions and failures.
- [x] Verify raw Event Log visibility and service status transitions.
- [x] Execute install → start → observe logs → stop → delete on a Windows test VM.
- [~] Verify service binary/configuration ACLs under the selected
    least-privilege account; binary read/execute passed, while ProgramData ACL
    provisioning remains because the default configuration path was absent.
- [x] Verify upgrade, rollback, and removal leave no stale service process or
    configuration behind.

**Gate:** Recovery does not trigger for intentional stop and does not create a tight restart loop.

## Phase 2 validation — live KVM

### V1. Guest Agent probe

- [x] Requires a running RHEL/libvirt host and Windows KVM guest; read-only
    checks completed against `win11_gpu` on 2026-08-18.
- [x] Run host prerequisite checks.
- [x] Confirm the virtio-serial channel name `org.qemu.guest_agent.0`.
- [x] Confirm QGA service availability and permissions in Windows; `qemu-ga`
    was observed running and completed a controlled stop/start cycle.
- [x] Run `guest-info` and the configured host memory-stat source at least
    three times; `guest-info` succeeds, QGA `110.0.2` does not provide
    the nonstandard `guest-get-memory-stats` extension, and M9e now validates
    `dommemstat` correctness/freshness before policy use.
- [x] Record QEMU, libvirt, QGA versions, command latency, and observed
    response fields; libvirt 11.10.0, QEMU API 11.10.0, hypervisor 10.1.0,
    QGA 110.0.2, `guest-info` at 77–124 ms, and `dommemstat` at 84–129 ms
    were captured on the RHEL host.
- [x] Confirm the host-facing channel path/state and controller access; the
    Windows service uses native telemetry and does not open the QGA pipe.
- [x] Repeat the probe after an isolated QGA restart and graceful guest
    reboot; QGA, fallback stats, connected channel, and convergence recovered.

### V2. Live virtio-mem inspection

- [x] Capture the virtio-mem alias and block size from live XML (`ua-virtiomem0`,
  2 MiB).
- [x] Capture the fresh `requested=0 KiB`, `current=0 KiB`, and
    `size=20971520 KiB` state (fully unplugged and converged, with a 20 GiB
    device maximum).
- [x] Confirm the 2 MiB block size matches the host's 2 MiB THP PMD size.
- [x] Confirm through live QMP that `dynamic-memslots=true` and
    `unplugged-inaccessible=on` are in use.
- [x] Select a reversible, aligned target within configured limits (1 GiB).
- [x] Confirm the installed controller sends one request and waits for
    convergence before resuming policy evaluation.
- [x] Confirm the VM and workload review: VFIO devices are GPU/audio/USB rather
    than NVMe, `mem-lock=off`, and no RDMA or unsupported vhost-user dependency
    is present or intended.

### V3. Single-VM failure and recovery

- [ ] Reuse any suitable M10a3 evidence; do not schedule a duplicate live
    resize merely to satisfy this gate.
- [ ] Confirm every failure/recovery case preserves the convergence and
    no-overlap rules.
- [ ] Test QGA interruption, guest reboot, failed update, and service restart.
- [ ] Qualify exact-target re-notification and the one-shot
    abandon-to-current recovery against both no-progress and partial-progress
    Windows shrink states.
- [ ] Preserve evidence and update API/issue documentation with observed behavior.
- [ ] Verify host and guest logs can correlate one policy decision to one host
    request and one convergence result.

**Live validation gate:** No automatic memory updates until V1 and V2 pass.

## Phase 3 — Global arbitration and controlled reclaim

Phase 3 simulation starts after the Phase 2 demand contract is trustworthy;
it does not wait for optional Windows kernel tracing. The Linux global controller becomes
the sole owner of host reserve accounting, VM pool capacity, and multi-VM
allocation decisions. The Windows service remains a measurement and
recommendation agent; it does not issue Linux/libvirt commands or direct
`viomem.sys` requests.

### G0. Demand integration prerequisites

- [x] **M9d:** bind workload compatibility authorization to a fingerprint of
    the complete reviewed live domain/QEMU configuration and revoke it on
    drift.
- [x] **M9e:** correct `dommemstat` balloon semantics, validate source
    freshness, and replace the QGA-only Bash preview with the Rust controller
    decision path.
- [x] **M10c:** implement and test the selected host-side join of fresh raw
    guest telemetry with alias-scoped live libvirt `current`. Windows must not
    infer allocation, receive an allocation feed, or invoke host tools.
- [~] **M10d:** version the report envelope with VM/service/session identity,
    wall-clock and monotonic ordering, sequence/correlation, allocation
    provenance, and freshness/replay rules.
- [ ] Bound any filesystem delivery with least-privilege ACLs, maximum record
    and file sizes, retention/rotation, partial-write recovery, and explicit
    reader handoff.

**Gate:** The global controller rejects stale, replayed, cross-VM, incomplete,
or provenance-free demand input before evaluating policy.

### G1. Allocation contract and diagnostic evidence

- [x] **M10a/M10a4:** pin the Virtio and implementation-source mapping and make
    alias-scoped live libvirt `current` authoritative for host accounting.
- [x] **M10a1, optional:** qualify checksum-recorded bounded kernel-debug
    capture as an installed-driver diagnostic, accounting for debug-message
    filtering and capture-driver/process/artifact cleanup.
- [x] **M10a2:** define and hermetically test a versioned correlated behavior
    format that fails closed on missing required host/Windows-health layers,
    ambiguous units, mixed operations, or incomplete convergence; accept
    driver trace as optional diagnostic evidence.
- [x] **M10a3, optional:** after disposable-guest rehearsal when practical,
    run one separately approved bounded observation with an explicit recovery
    target and without assuming that a shrink is guaranteed rollback.
- [x] **M10aX, conditional:** the observed no-progress/partial-progress shrink
    stalls and empty bounded captures establish the unmet diagnostic need. The
    separate feasibility proposal specifies—but does not implement—a versioned
    read-only driver status interface with security, compatibility, test,
    external build/signing/install, and rollback gates.

### G2. Global RAM pool model

- [ ] Model total physical RAM, fixed host baseline, cache allowance,
    emergency reserve, VM capacity, actual VM allocations, and pool-free RAM.
- [ ] Count observed active/plugged allocation, not requested allocation, as
    capacity consumed during convergence.
- [ ] Fail closed when demand reports or actual VM state are stale, missing, or
    internally inconsistent.

### G3. Multi-VM arbitration simulation

- [ ] Add independent growth and reclaim priorities for every VM.
- [ ] Simulate `NORMAL`, `CAUTION`, `PRESSURE`, `CRITICAL`, and `EMERGENCY`
    transitions with hysteresis and block-sized decisions.
- [ ] Respect configured minimums, advisory safe floors, in-flight operations,
    stale reports, host reserve, and pool capacity.
- [ ] Prove the policy deterministically before connecting live multi-VM
    actuation.

### G4. Controlled reclaim and actuation

- [ ] Add rolling demand history and conservative, validated safe floors.
- [ ] Reclaim one aligned step at a time, wait for convergence, and stop on
    pressure or incomplete evidence.
- [ ] Reuse the M10b immutable-target, retry-budget, hard-deadline, latched
    stall, and abandon-to-current rules; do not invent a separate global-pool
    retry mechanism.
- [ ] Keep direct `viomem.sys` IOCTLs deferred unless a separate supported
    interface, security, signing, timeout, and rollback investigation passes.

**Phase 3 gate:** Pool accounting and arbitration are deterministic,
observable, bounded, and based on actual observed virtio-mem state; simulated
reclaim passes before any automatic multi-VM live action.

## Phase 4 — Hardening

### H1. Error handling and recovery

- [~] Classify expected cancellation, transient transport failure, invalid data, and fatal worker failure; cancellation and fatal SCM failures are distinct, while transient retry classes remain open.
- [x] Add bounded in-flight shutdown handling.
- [x] Verify non-zero failure exit behavior for SCM recovery.
- [ ] Add regression tests for restart and recovery decisions.
- [~] Use the selected M10b 30/60/120-second same-target schedule, maximum of
    three re-notifications, and immutable 300-second deadline for Windows
    shrink qualification; transport failures remain separately classified and
    no resize may be retried blindly.
- [x] Verify intentional stop, startup failure, and unexpected worker exit have
    distinct exit/recovery behavior; system-shutdown live evidence remains part
    of the wider recovery matrix.

### H2. Logging and observability

- [~] Emit structured lifecycle, telemetry, policy, resize, and shutdown events; Windows lifecycle/failure events exist, while demand and host correlation remains open.
- [x] Integrate Windows Event Log for SCM lifecycle and failure events.
- [x] Avoid logging secrets or raw sensitive configuration in the implemented Windows event sink.
- [ ] Add useful correlation/context fields for failed requests.
- [ ] Define log volume limits and redaction rules for paths, account names, and
    configuration values.

### H3. Configuration persistence

- [~] Use the documented ProgramData configuration location; least-privilege ACL provisioning remains open.
- [x] Load persisted values with validation and safe defaults.
- [ ] Reject unsafe account, endpoint, interval, and limit values.
- [x] Test missing, malformed, and partially specified configuration.
- [ ] Test file/registry ACLs and atomic update/rollback behavior.

### H4. Performance and safety tuning

- [~] Measure QGA response latency and polling overhead; live host-side QGA
    latency is recorded, while native telemetry overhead remains open.
- [ ] Tune hysteresis using observed memory pressure behavior.
- [~] Confirm no overlapping polls or resize requests; M9b observed no
    overlapping resize, while the wider failure/restart matrix remains M10b.
- [ ] Verify bounded shutdown under slow QGA responses.
- [ ] Set explicit latency and shutdown acceptance thresholds from measured KVM
    results rather than assumptions.

**Phase 4 gate:** Failures are actionable, observable, bounded, and covered by local tests.

## Phase 5 — Operations

### O1. Host automation

- [x] Make host scripts and the Rust CLI validate explicit VM names and prerequisites.
- [x] Add safe inspection/reporting for live XML and convergence.
- [x] Keep resize actions opt-in and explicitly scoped.

### O2. Health and monitoring

- [ ] Add service health state and last-success timestamps.
- [ ] Add monitoring/alerting guidance for QGA loss, stale metrics, and resize failure.
- [ ] Document operator response and rollback steps.

### O3. Release readiness

- [x] Produce a repeatable Windows build/publish procedure.
- [~] Produce install, upgrade, rollback, and removal procedures; install,
    removal, and one rollback are evidenced, while upgrade packaging remains.
- [x] Complete the current documentation freshness check; repeat on every change.
- [x] Record known platform/version compatibility for the validated VM stack.
- [ ] Produce a versioned release artifact with checksum and dependency/license
    inventory.
- [ ] Define rollback criteria and a safe disable path before enabling automatic
    resize.

## Dependency path

```text
M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7
              └──────────────→ M8 → M9 → M9a → M9b → M9d ─────────┐
                                            └──────→ M9e ──────────┤
M8 + M9a → M10a (contract) → M10a4 ────────────────────────────────┤
M8 + M9a → M10a2 ───────────────→ M10b ────────────────────────────┼→ M11 → M11a → M12 → M13
M10a1 (optional diagnostics) → M10a3 (optional observation)         │
M4 + M6 → M10 ─────────────→ M10c → M10d ─────────────────────────┘
Concrete unmet diagnostic need only → M10aX
```

The live KVM path (`M8`) is external to the Windows build path. The Phase 2
demand-agent work (`M10`) can be developed with deterministic native-API fakes.
M10a2 is deliberately hermetic and does not wait for privileged M10a1 capture.
Hermetic global-pool simulation can begin from the completed M10a allocation
contract once M9e and M10d provide trustworthy inputs. Live multi-target
actuation cannot pass until M9d guards compatibility drift and M10b records
failure/recovery evidence. M10a1/M10a3 are optional diagnostics, and M10aX is
a conditional branch for a concrete unmet diagnostic need, not permission to
begin driver work. M9c has removed the duplicate Bash host-control
implementation before live resize automation is expanded.

## Active blockers and decisions

| ID | Blocker or decision | Impact | Owner/action |
| --- | --- | --- | --- |
| B13 | Native Windows telemetry and the versioned demand-report contract lack live workload evidence | Blocks production tuning and global-controller inputs, but not Windows service startup | Collect live workload evidence for `GlobalMemoryStatusEx`/`GetPerformanceInfo` reports without changing host actuation authority |
| B14 | The protocol/source mapping is established, but installed-driver notification and branch behavior are not directly observable through a supported user-mode API | Does not block host accounting or simulation; reduces diagnosis when a Windows operation stalls | Use optional bounded tracing only when its diagnostic value justifies protected-guest mutation |
| B15 | The signed Windows `viomem.sys` state message is kernel-debug output and informational debug prints may be filtered before capture | Optional DbgView evidence may be absent or ambiguous without persistent debug configuration changes | Qualify filtering and cleanup without boot logging, registry mutation, driver restart, or reboot; stop rather than escalate automatically |
| B17 | The M10d v2 envelope and read-side replay/size checks are implemented, but JSON-lines output lacks durable acknowledgement, restart-safe replay state, ACL provisioning, and bounded handoff/retention/rotation | Blocks restart-safe Phase 3 ingestion and still risks lost, unacknowledged, or unbounded producer output | Complete M10d with bounded durable-delivery rules and least-privilege deployment |
| B19 | Runtime failure injection does not yet cover the selected bounded Windows-shrink state machine or the full active-controller recovery matrix | Shrink can remain divergent without a proven same-target wakeup; rejection, reboot, restart, cancellation, and non-disruptive abandon-to-current also lack sufficient evidence | Implement the default-off controls; prove the 30/60/120-second, three-re-notification, 300-second policy and one-shot recovery under M10b before automated reclaim |
| B21 | Phase 2 instances have no atomic global host reservation | Multiple active controllers/devices can race the same host headroom | Support one active development controller/device until M11 arbitration |

Resolved blockers B4 (configuration location/format), B7 (unit boundaries),
B16 (host-side current-allocation join),
and B9 (bounded shutdown enforcement) are retained in Git history rather than
the active table. Former B5 was replaced by B16 because a guest resize sink
would violate the architecture.

## Definition of done for the project

The project is complete only when:

1. The Rust executable is installed and controlled by Windows SCM.
2. Start, stop, shutdown, failure, and recovery states are observable.
3. Configuration is persistent, validated, and least privilege by default.
4. Native Windows demand metrics are collected reliably, while host-side QGA
   health and `dommemstat` fallback behavior are independently validated.
5. Resize requests are aligned, convergent, bounded, and reversible.
6. Host and guest tests cover the install/start/stop/remove and live resize flows.
7. All QGA operations and shutdown paths have bounded deadlines.
8. Unit conversions and adapter contracts are tested end to end.
9. Native demand reports are versioned, canonical-byte based, bounded, and
    advisory; freshness, identity, provenance, retention, and replay behavior
    are enforced, and reports cannot directly actuate host memory.
10. Global pool accounting uses host reserve and observed actual VM allocation,
    and arbitration covers growth, reclaim, stale data, in-flight operations,
    and explicit pressure states.
11. Documentation, recovery procedures, health checks, release evidence, and
    known limitations are current.

## Known risks

- QEMU Guest Agent availability and Windows virtio-serial permissions.
- Memory allocation hysteresis tuning under real workload pressure.
- Windows shrink remaining divergent without a periodic retry event.
- Independent Phase 2 controllers racing host reserve; only one is supported.
- Uncontained unplugged memory for untrusted guests; hard QEMU/libvirt limits
  are mandatory outside trusted development use.
- Unbounded or replayed demand records until M10d is complete.
- Slow or interrupted QGA responses during bounded shutdown.
- SCM callback timing and recovery semantics.
- Cross-platform integration between the Windows guest and Linux KVM host.

## Revised implementation direction

The implementation is deliberately staged:

- **Phase 2 demand-agent foundation:** add native Windows telemetry and a
  versioned demand recommendation while retaining the current one-VM host
  resize path as the only actuation authority.
- **Phase 3 global controller:** add host reserve accounting, multi-VM
  arbitration, driver/QEMU state reconciliation, trend-aware reclaim, and
  controlled operational rollout.

### Phase 2 demand-agent milestones

- [x] Collect physical memory with `GlobalMemoryStatusEx`.
- [x] Collect commit/system metrics with `GetPerformanceInfo`.
- [x] Define canonical-byte raw snapshots and a versioned demand report.
- [x] Calculate bounded pressure ratios and provisional demand states.
- [x] Produce desired-target and safe-floor recommendations without issuing
    virtio-mem changes from the Windows service.
- [x] Preserve upstream QGA health/identity and freshness-qualified
    `dommemstat` compatibility until native telemetry has live evidence.
- [x] Add deterministic tests for invalid counters, ratio bounds, target
    limits, and alignment.

**Phase 2 gate:** The guest can report a complete demand snapshot locally, and
the existing host controller remains the only resize authority.

### Phase 3 global-controller milestones

- [x] Establish allocation authority from Virtio and pinned implementation
    sources: alias-scoped live libvirt `current` is authoritative, while driver
    trace is optional diagnostic evidence.
- [ ] Model total RAM, host reserve, VM capacity, and actual pool-free memory.
- [ ] Add separate growth and reclaim priorities for each VM.
- [ ] Simulate `NORMAL`, `CAUTION`, `PRESSURE`, `CRITICAL`, and `EMERGENCY`
    arbitration states before connecting live multi-VM actuation.
- [ ] Add rolling history, conservative safe floors, bounded aligned reclaim,
    convergence waits, and stop-on-pressure behavior.
- [ ] Treat direct viomem IOCTLs as deferred research until source, signing,
    permissions, and runtime behavior are proven.
- [ ] Keep any viomem fork/build/signing/install work as a separate kernel
    driver track with its own rollback and disposable-guest validation.

**Phase 3 gate:** Pool accounting and reclaim are deterministic, observable,
bounded, and based on actual observed virtio-mem state.
