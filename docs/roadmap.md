# Roadmap

## Status legend

- `[ ]` Not started
- `[~]` In progress or partially complete
- `[x]` Complete with local evidence and documentation
- `[!]` Blocked by an external dependency

## Current position

The project is in **Phase 2: Core Functionality**. The parser, resize policy,
QEMU Guest Agent client boundary, wakeable polling loop, portable service host,
validated configuration model, startup validation path, and local Windows
SCM install/stop registration path are implemented and locally tested. The
workspace also contains a host-side virtio-mem controller scaffold with
XML/state validation and a bounded runtime loop. The next implementation
priority is the Phase 2 Windows demand-agent foundation: native telemetry,
canonical-byte demand reports, and bounded target recommendations that do not
take over host actuation. The existing one-VM host controller remains the only
resize authority until live state mapping and Phase 3 global arbitration have
been validated. M8 live QGA/KVM validation is complete on `win11_gpu`, including
isolated agent restart and graceful guest reboot recovery.
The QGA memory command may be unavailable on the guest, but that is no longer
a Windows service startup blocker because the service uses native
`GlobalMemoryStatusEx` and `GetPerformanceInfo` telemetry. The host controller
uses `dommemstat` by default when the guest QGA does not provide
`guest-get-memory-stats`.

## Recent verified wins

- **Local quality baseline:** on 2026-08-18 the workspace passed
    `cargo test --workspace --all-features`, `cargo clippy --workspace
    --all-targets --all-features -- -D warnings`, and `cargo build --workspace
    --all-features`; the workspace currently reports 77 tests passing and 0
    failures.
- **Safe policy core:** resize decisions are aligned, bounded by configured
    limits, hysteresis-aware, and blocked while `requested != current`.
- **Strong boundary separation:** guest Rust code does not invoke Linux commands;
    host Bash validation stays explicit-scope and read-only by default.
- **Failure visibility:** parser, transport, startup, polling, resize, and
    worker failures return typed errors instead of silent fallback.
- **Cancellation correctness:** stop wakes the polling wait rather than
    delaying shutdown for the full interval.
- **Host/controller scaffolding:** the RHEL host adapter validates live XML,
    selected alias data, QGA responses, and resize requests before sending a
    change, and the runtime loop blocks overlapping updates until convergence.
- **Documentation traceability:** architecture, API, testing, backlog, and
    roadmap status are updated together with implementation work.
- **Host-side virtio-mem reality check:** libvirt/QEMU guidance confirms a
    virtio-mem resize is an asynchronous `requested` change, not an immediate
    guest memory state switch; the controller must wait for convergence before
    issuing a follow-up request.
- **Host stats-source fallback and hard safety invariants (2026-08-18):** the
    host controller can now source memory stats from `virsh dommemstat`
    instead of the unimplemented `guest-get-memory-stats`, and a shared
    `MIN_HEADROOM_BYTES` invariant plus a host `/proc/meminfo` headroom gate
    are enforced in code rather than only by configuration. Live systemd
    installation and a resize test through the installed service remain.
- **RHEL read-only fallback evidence (2026-08-18):** `virsh dommemstat
    win11_gpu` returned `actual`, `unused`, and `available` successfully, so
    the default host stats source is observable on this guest. Live XML remains
    the authoritative source for the requested/current convergence gate.
- **RHEL service observation (2026-08-18):** the read-only check found no
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
    for `dommemstat`. QGA still does not implement
    `guest-get-memory-stats`; the verified fallback remains authoritative.
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
- **Fresh convergence recheck (2026-08-18):** `win11_gpu` still reports
    `requested=0 KiB` and `current=0 KiB` after the latest Windows driver
    update. The previous rollback convergence blocker is resolved. The QGA
    responses checked here do not expose Windows driver
    `requested_size`/`plugged_size`; the cross-layer driver mapping remains
    unverified.
- **Windows service running handoff (2026-08-18):** RHEL-side QGA checks
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
  compile Windows SCM APIs for Linux. Current evidence is a release build, 38
  passing core/host tests, rustfmt, warnings-as-errors Clippy, and Bash syntax.
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
- [ ] **Stage F — Optional live validation:** execute Windows SCM lifecycle and
  RHEL systemd/libvirt/QGA tests under their existing separate approval and
  rollback procedures. These tests are release evidence, not part of the
  default non-mutating developer gate.

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

- **Local quality baseline:** the native Windows MSVC build, release build,
    77 unit tests, and Clippy warnings-as-errors gate pass locally.
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
- QEMU does not provide a balloon-style protection layer for unplugged memory;
    cgroups or similar host-side limits are still required to control VM memory
    consumption.
- `dynamic-memslots=on` is recommended when available and must be combined with
    `unplugged-inaccessible=on` for the virtualization stack to treat unplugged
    blocks as inaccessible.
- Some features and device types remain incompatible with virtio-mem, including
    `vdpa`, `RDMA migration`, `vfio-nvme`, `mlock`-based setups, and several
    vhost-user cases such as DPDK/SPDK.
- For virtio-mem memory backends, sparse semantics are expected: `reserve=off`
    and `prealloc=off` for the backend, while `prealloc=on` is sometimes used on
    the virtio-mem device itself.

These findings should be treated as the baseline for live validation and release
readiness in the remaining host-side work.

## Milestone map

| ID | Milestone | Status | Depends on | Exit evidence |
| --- | --- | --- | --- | --- |
| M0 | Repository and architecture baseline | [x] | — | Architecture, contracts, standards, and testing docs reviewed |
| M0a | RHEL-controlled cross-platform developer gate | [x] | M0 | RHEL core/host gate and two fingerprint-pinned aggregate native runs pass; both produce the same checksum-verified Windows executable |
| M1 | Pure memory policy and QGA parsing | [x] | M0 | Parser and controller tests cover malformed, boundary, alignment, and convergence cases |
| M2 | Guest runtime polling foundation | [x] | M1 | Poller, named-pipe client boundary, wakeable scheduler, and transport/error tests pass locally; operation deadlines remain |
| M3 | Service lifecycle foundation | [x] | M2 | Startup readiness, cancellation, failure, state, and bounded shutdown tests pass locally; real SCM observation remains |
| M4 | Runtime configuration foundation | [x] | M2 | Versioned JSON schema, persistent loading, identity, endpoint, demand-report path, timing, account, missing-file defaults, and validation model exist locally; ACL provisioning remains |
| M5 | Native Windows SCM adapter | [x] | M3, M4 | Elevated Program Files lifecycle passed under LocalService with stable live Event Log records, bounded callbacks, clean-stop exit zero, and failure exit one |
| M6 | Concrete guest runtime wiring | [~] | M4, M5 | Interactive and SCM paths now collect native Windows telemetry without opening the QGA device; trustworthy current-allocation and resize wiring remain |
| M7 | Installation and recovery operations | [x] | M5, M6 | Live install/start/observe/stop/delete passed; 5-second recovery restart and 5/30/60 metadata were verified, rollback restored the original running service |
| M8 | Live QGA and KVM validation | [x] | M2 | Repeated QGA and `dommemstat` probes, connected-channel XML, isolated QGA restart recovery, graceful guest reboot recovery, and unchanged convergence all passed on `win11_gpu` |
| M9 | Host virtio-mem XML adapter | [x] | M1, M8 | Live Rust CLI snapshot/validation, exact alias selection, canonical zero-state parsing, wrong-alias rejection, fail-closed dry run, and before/after non-mutation evidence pass on `win11_gpu` |
| M9a | Virtio-mem safety and compatibility gate | [x] | M8, M9 | Fresh live QMP properties, THP/block match, explicit operator review, VFIO device classification, locked/RDMA/vhost-user exclusion, exact dry run, and XML non-mutation passed on `win11_gpu` |
| M9b | RHEL systemd host controller | [x] | M1, M8, M9, M9a | Installed one-VM systemd controller completed a guarded zero-to-1-GiB bootstrap on `win11_gpu`, converged, retained its minimum, and remains active without overlapping requests |
| M9c | Rust host CLI replaces Bash resize helper | [x] | M9, M9a | Rust owns snapshot, validation, exact dry-run arguments, and explicitly applied resize commands; hermetic regression tests pass and the duplicate Bash helper is removed |
| M10 | Phase 2 demand-agent foundation | [~] | M4, M6 | Native Windows telemetry, versioned demand report, bounded pressure state, desired target, advisory safe floor, durable JSON-lines output, and generic stoppable worker are locally tested; main SCM construction, trustworthy allocation provider, and live workload evidence remain; no direct host actuation |
| M10a | Cross-layer state observation | [ ] | M8, M9, M9a | Controlled evidence maps driver `requested_size`/`plugged_size` to QEMU/libvirt `requested`/`current` without treating the fields as interchangeable by assumption |
| M10b | End-to-end single-VM resize flow | [ ] | M7, M8, M9, M9a, M9b | One reversible aligned resize converges without overlapping requests and records the observed state transition |
| M11 | Phase 3 global pool arbitration | [ ] | M10, M10a, M10b | Hermetic multi-VM simulation models host reserve, actual allocations, pool-free capacity, growth/reclaim priorities, stale reports, and all five pressure states |
| M11a | Controlled reclaim and convergence | [ ] | M11 | Trend-aware safe floors, bounded aligned reclaim, hysteresis, in-flight protection, convergence waits, and stop-on-pressure behavior pass simulation tests |
| M12 | Hardening and observability | [ ] | M11a | Recovery, event logging, metrics, bounded timeout behavior, and restart tests pass for guest and global-controller paths |
| M13 | Operational release readiness | [ ] | M12 | Documentation, health checks, monitoring, compatibility evidence, rollback, and repeatable host automation complete |

## Phase 1 — Foundation

### F1. Repository baseline — complete

- [x] Rust 2021/MSVC project structure established.
- [x] Rust-only runtime and Bash-only automation boundaries documented.
- [x] API, data model, architecture, engineering, and testing documents created.
- [x] Backlog and validation scripts established.

**Gate:** Documentation and repository rules are present before runtime changes.

### F2. QEMU Guest Agent contract — complete

- [x] `guest-info` and `guest-get-memory-stats` request/response contract documented.
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

### F6. Service lifecycle and configuration — partially complete

- [x] `ServiceHost` models startup readiness, running, stopping, stopped, and failed states.
- [x] Startup failures are distinct from runtime worker failures.
- [x] Stop and shutdown share one wakeable cancellation path.
- [x] Service identity, QGA endpoint, poll interval, shutdown timeout, and least-privilege account defaults are validated.
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
- [~] Add compatibility checks for `dynamic-memslots`/`unplugged-inaccessible`
    and known incompatible device classes before enabling live automation;
    XML flags now parse as confirmed/rejected/unknown and unknown state blocks
    the resize sink, while workload/device review remains operator evidence.

**Gate:** A target size can be traced from QGA observation to host request with
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
- [~] Implement guest-side demand/state acquisition; native telemetry is
    collected and validated at worker initialization, but it does not establish
    virtio-mem `current` allocation.
- [ ] Implement a safe resize-request sink without Linux command execution;
    defer until a trustworthy current-allocation provider exists.
- [x] Add structured error context at the service boundary for configuration,
    worker construction/initialization, and service-host execution failures.
- [x] Add a deterministic fake state provider and resize sink for integration
    tests; local fakes now provide validated byte snapshots, while the live XML
    state provider and production resize sink remain.

**Gate:** The executable can start its worker, stop cleanly, and fail visibly when an adapter fails.

### F8a. Failure-injection and contract harness

- [~] Simulate QGA timeout, disconnect, malformed JSON, partial response, and
    stale data without a live VM.
- [~] Simulate resize rejection, non-convergence, guest reboot, and service
    restart through fakes.
- [~] Verify no resize is issued after cancellation or while a request is
    pending.
- [x] Keep the harness independent of Linux tools and production VM state.

**Gate:** Every failure mode in the service boundary has deterministic local
evidence before live testing.

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
    `guest-get-memory-stats`, and `dommemstat` is the verified fallback.
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

### V3. End-to-end resize

- [ ] Perform one manual reversible live resize.
- [ ] Confirm convergence before a second request.
- [ ] Test QGA interruption, guest reboot, failed update, and service restart.
- [ ] Preserve evidence and update API/issue documentation with observed behavior.
- [ ] Verify host and guest logs can correlate one policy decision to one host
    request and one convergence result.

**Live validation gate:** No automatic memory updates until V1 and V2 pass.

## Phase 3 — Global arbitration and controlled reclaim

Phase 3 starts only after the Phase 2 demand-agent gate and the live
cross-layer state-observation gate pass. The Linux global controller becomes
the sole owner of host reserve accounting, VM pool capacity, and multi-VM
allocation decisions. The Windows service remains a measurement and
recommendation agent; it does not issue Linux/libvirt commands or direct
`viomem.sys` requests.

### G1. Cross-layer state mapping

- [ ] Capture controlled, reversible observations of Windows
    `requested_size`/`plugged_size` and QEMU/libvirt `requested`/`current`.
- [ ] Document which values represent requested state versus actual active
    allocation and which values may remain stale during convergence.
- [ ] Do not use driver and libvirt field names interchangeably until the
    mapping is proven across the same resize operation.

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
- [ ] Keep direct `viomem.sys` IOCTLs deferred unless a separate supported
    interface, security, signing, timeout, and rollback investigation passes.

**Phase 3 gate:** Pool accounting and arbitration are deterministic,
observable, bounded, and based on actual observed virtio-mem state; simulated
reclaim passes before any automatic multi-VM live action.

## Phase 4 — Hardening

### H1. Error handling and recovery

- [ ] Classify expected cancellation, transient transport failure, invalid data, and fatal worker failure.
- [ ] Add bounded in-flight shutdown handling.
- [ ] Verify non-zero failure exit behavior for SCM recovery.
- [ ] Add regression tests for restart and recovery decisions.
- [ ] Define transient-error backoff and a maximum retry budget; never retry a
    resize blindly.
- [ ] Verify intentional stop, shutdown, startup failure, and worker crash have
    distinct exit/recovery behavior.

### H2. Logging and observability

- [ ] Emit structured lifecycle, QGA, policy, resize, and shutdown events.
- [ ] Integrate Windows Event Log or an equivalent documented sink.
- [ ] Avoid logging secrets or raw sensitive configuration.
- [ ] Add useful correlation/context fields for failed requests.
- [ ] Define log volume limits and redaction rules for paths, account names, and
    configuration values.

### H3. Configuration persistence

- [ ] Select and document the Windows configuration location and permissions.
- [ ] Load persisted values with validation and safe defaults.
- [ ] Reject unsafe account, endpoint, interval, and limit values.
- [ ] Test missing, malformed, and partially specified configuration.
- [ ] Test file/registry ACLs and atomic update/rollback behavior.

### H4. Performance and safety tuning

- [ ] Measure QGA response latency and polling overhead.
- [ ] Tune hysteresis using observed memory pressure behavior.
- [ ] Confirm no overlapping polls or resize requests.
- [ ] Verify bounded shutdown under slow QGA responses.
- [ ] Set explicit latency and shutdown acceptance thresholds from measured KVM
    results rather than assumptions.

**Phase 4 gate:** Failures are actionable, observable, bounded, and covered by local tests.

## Phase 5 — Operations

### O1. Host automation

- [ ] Make host scripts validate explicit VM names and prerequisites.
- [ ] Add safe inspection/reporting for live XML and convergence.
- [ ] Keep resize actions opt-in and explicitly scoped.

### O2. Health and monitoring

- [ ] Add service health state and last-success timestamps.
- [ ] Add monitoring/alerting guidance for QGA loss, stale metrics, and resize failure.
- [ ] Document operator response and rollback steps.

### O3. Release readiness

- [ ] Produce a repeatable Windows build/publish procedure.
- [ ] Produce install, upgrade, rollback, and removal procedures.
- [ ] Complete documentation freshness checks.
- [ ] Record known platform/version compatibility.
- [ ] Produce a versioned release artifact with checksum and dependency/license
    inventory.
- [ ] Define rollback criteria and a safe disable path before enabling automatic
    resize.

## Dependency path

```text
M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7
              └──────────────→ M8 → M9 → M9a → M9b ─┐
                                                   ├→ M10 → M10a → M10b → M11 → M11a → M12 → M13
              M4 → M6 ────────────────────────────┘
```

The live KVM path (`M8`) is external to the Windows build path. The Phase 2
demand-agent work (`M10`) can be developed with deterministic native-API fakes,
but the global-controller path cannot pass its gates until state mapping and
single-VM convergence evidence are available. M9c has removed the duplicate
Bash host-control implementation before live resize automation is expanded.

## Active blockers and decisions

| ID | Blocker or decision | Impact | Owner/action |
| --- | --- | --- | --- |
| B4 | Persistent configuration location and format are not selected | Blocks production startup configuration | Choose a Windows-safe, least-privilege configuration mechanism in H3 |
| B5 | Concrete guest state and resize sinks are not wired | Blocks real automatic resize behavior | Implement M6 without invoking Linux commands from the guest |
| B7 | QGA, controller, libvirt, and `virsh` memory-unit semantics are not reconciled in one tested contract | Blocks safe resize enablement | Resolve in F6a before M9/M10 |
| B9 | Shutdown timeout is configured but not yet enforced by the worker host | Stop-pending behavior cannot be proven | Add bounded join/worker termination policy in M3/M5 |
| B13 | Native Windows telemetry and the versioned demand-report contract lack live workload evidence | Blocks production tuning and global-controller inputs, but not Windows service startup | Collect live workload evidence for `GlobalMemoryStatusEx`/`GetPerformanceInfo` reports without changing host actuation authority |
| B14 | Driver `plugged_size` versus libvirt `current` has not been validated as one cross-layer state mapping | Blocks global pool accounting and safe reclaim | Capture the same controlled resize through driver, QEMU, and libvirt observation before treating actual allocation as interchangeable |

## Definition of done for the project

The project is complete only when:

1. The Rust executable is installed and controlled by Windows SCM.
2. Start, stop, shutdown, failure, and recovery states are observable.
3. Configuration is persistent, validated, and least privilege by default.
4. QGA metrics are collected reliably on the Windows KVM guest.
5. Resize requests are aligned, convergent, bounded, and reversible.
6. Host and guest tests cover the install/start/stop/remove and live resize flows.
7. All QGA operations and shutdown paths have bounded deadlines.
8. Unit conversions and adapter contracts are tested end to end.
9. Native demand reports are versioned, canonical-byte based, bounded, and
    advisory; they cannot directly actuate host memory.
10. Global pool accounting uses host reserve and observed actual VM allocation,
    and arbitration covers growth, reclaim, stale data, in-flight operations,
    and explicit pressure states.
11. Documentation, recovery procedures, health checks, release evidence, and
    known limitations are current.

## Known risks

- QEMU Guest Agent availability and Windows virtio-serial permissions.
- Memory allocation hysteresis tuning under real workload pressure.
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
- [ ] Preserve QGA/dommemstat compatibility until native telemetry has live
    evidence.
- [x] Add deterministic tests for invalid counters, ratio bounds, target
    limits, and alignment.

**Phase 2 gate:** The guest can report a complete demand snapshot locally, and
the existing host controller remains the only resize authority.

### Phase 3 global-controller milestones

- [ ] Reconcile driver `requested_size`/`plugged_size` with libvirt
    `requested`/`current` using controlled evidence.
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
