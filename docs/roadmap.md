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
XML/state validation and a bounded runtime loop. The remaining gaps are
primarily live KVM validation, real Windows guest plumbing, and service
registration/operation proof on a real guest rather than a local synthetic
test harness.

## Recent verified wins

- **Local quality baseline:** on 2026-08-18 the workspace passed
    `cargo test --workspace --all-features`, `cargo clippy --workspace
    --all-targets --all-features -- -D warnings`, and `cargo build --workspace
    --all-features`; the workspace currently reports 46 tests passing and 0
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

## Verified wins to preserve

- **Local quality baseline:** the native Windows MSVC build, release build,
    46 unit tests, and Clippy warnings-as-errors gate pass locally.
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
| M1 | Pure memory policy and QGA parsing | [x] | M0 | Parser and controller tests cover malformed, boundary, alignment, and convergence cases |
| M2 | Guest runtime polling foundation | [x] | M1 | Poller, named-pipe client boundary, wakeable scheduler, and bounded-I/O tests pass locally |
| M3 | Service lifecycle foundation | [x] | M2 | Startup readiness, cancellation, bounded shutdown, failure, and state tests pass locally |
| M4 | Runtime configuration foundation | [x] | M2 | Versioned persistent schema, identity, endpoint, timing, account, and validation model exists locally |
| M5 | Native Windows SCM adapter | [~] | M3, M4 | SCM dispatcher and local Windows install/stop registration path are implemented; real guest/service-manager validation remains |
| M6 | Concrete guest runtime wiring | [~] | M4, M5 | `main.rs` starts the configured worker and maps unexpected failures to a non-zero process result; live guest state/resize sink wiring still needs proof on a real VM |
| M7 | Installation and recovery operations | [ ] | M5, M6 | Install/start/observe/stop/delete sequence passes; bounded recovery actions are verified |
| M8 | Live QGA and KVM validation | [!] | M2 | Host probe succeeds repeatedly against the Windows KVM guest |
| M9 | Host virtio-mem XML adapter | [~] | M1, M8 | Captured XML alias/unit parsing, state validation, injectable XML state-provider boundary, and opt-in Bash live source/sink checks are implemented; live VM evidence remains |
| M9a | Virtio-mem safety and compatibility gate | [ ] | M8, M9 | Host adapter documents the QEMU/libvirt limits, dynamic memslot requirements, and incompatible device classes before any live resize automation |
| M9b | RHEL systemd host controller | [~] | M1, M8, M9, M9a | Shared Rust policy core and one-VM-per-instance systemd controller perform bounded QGA/XML/resize operations with no overlapping requests; live evidence remains |
| M10 | End-to-end resize flow | [ ] | M7, M8, M9, M9a, M9b | One reversible aligned resize converges without overlapping requests |
| M11 | Hardening and observability | [ ] | M10 | Recovery, event logging, metrics, timeout, and restart tests pass |
| M12 | Operational release readiness | [ ] | M11 | Documentation, health checks, monitoring, and repeatable host automation complete |

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
- [ ] Enforce bounded connect, write, flush, and read deadlines for QGA I/O.
- [ ] Prevent a slow or stuck QGA operation from exceeding the shutdown deadline.

**Gate:** Pure Rust tests pass and no host command is invoked by guest logic.

### F5. Guest transport boundary — partially complete / live validation pending

- [x] Configurable Windows named-pipe client sends newline-delimited QGA JSON.
- [x] Transport, empty-response, parser, and policy errors remain explicit.
- [ ] Confirm newline framing, response correlation, and malformed envelope handling against captured QGA traffic.
- [!] Validate the actual pipe path, permissions, QGA service, and response format on the Windows KVM guest.

**Gate:** Three consecutive read-only QGA probes succeed on the real VM.

### F6. Service lifecycle and configuration — partially complete

- [x] `ServiceHost` models startup readiness, running, stopping, stopped, and failed states.
- [x] Startup failures are distinct from runtime worker failures.
- [x] Stop and shutdown share one wakeable cancellation path.
- [x] Service identity, QGA endpoint, poll interval, shutdown timeout, and least-privilege account defaults are validated.
- [ ] Load persistent configuration rather than relying only on in-process defaults.
- [ ] Add a versioned configuration schema and migration/rejection rules.
- [ ] Enforce the configured shutdown timeout rather than only storing it.

**Gate:** Worker readiness precedes `Running`; expected cancellation is not a crash; unexpected failures remain recoverable by the SCM layer.

### F6a. Contract and unit safety — must-have before live resize

- [x] Choose bytes (`u64`) as the canonical internal memory unit and document
    every conversion boundary.
- [~] Reconcile QGA bytes, controller bytes, libvirt XML values, and any
    `virsh` command units before enabling a resize sink; the pure Rust
    `VirtioMemState` contract and captured XML parser are implemented, but live
    discovery and resize wiring remain.
- [~] Reject zero size, undersized/non-power-of-two block size, zero or
    out-of-range values, and unaligned values in the pure Rust contract; wire
    it into every host adapter boundary when the XML adapter is added.
- [~] Enforce `requested % block == 0`, `requested <= size`, and `block >= 1 MiB`
    checks in the pure Rust contract; wire those checks into the host XML
    adapter before issuing a resize request.
- [ ] Add boundary tests for maximum values and unit conversion round trips.
- [ ] Add compatibility checks for `dynamic-memslots`/`unplugged-inaccessible` and
    known incompatible device classes before enabling live automation.

**Gate:** A target size can be traced from QGA observation to host request with
no ambiguous or implicit unit conversion.

### F7. Native Windows service integration — in progress locally

- [~] Implement the Rust SCM dispatcher and service callback adapter.
- [~] Report `SERVICE_START_PENDING` with bounded wait hints/checkpoints.
- [~] Report `SERVICE_RUNNING` only after worker initialization succeeds.
- [~] Accept stop and system-shutdown controls and signal `StopSignal`.
- [~] Report `SERVICE_STOP_PENDING` during bounded shutdown, then `SERVICE_STOPPED`.
- [~] Return a non-zero process result for unexpected worker failure.
- [~] Add a local Windows service registration and stop path that uses the SCM
    APIs exposed by the currently installed `winapi` crate.
- [ ] Validate the install/start/stop lifecycle on a real Windows guest with
    service manager permissions and event-log visibility.

**Gate:** SCM lifecycle tests pass on Windows and callbacks remain bounded/non-blocking.

### F8. Concrete runtime wiring

- [~] Replace the `main.rs` foundation stub with service/interactive-mode dispatch.
- [ ] Connect `ServiceConfig` to `NamedPipeGuestAgent`.
- [ ] Implement guest-side memory state acquisition.
- [ ] Implement a safe resize-request sink without Linux command execution.
- [ ] Add structured error context at the service boundary.
- [~] Add a deterministic fake state provider and resize sink for integration
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

- [ ] Define stable service name, display name, description, executable path, startup mode, and account.
- [ ] Install with the least-privileged account that can access the QGA channel.
- [ ] Configure bounded restart delays only for unexpected failures.
- [ ] Verify event-log visibility and service status transitions.
- [ ] Execute install → start → observe logs → stop → delete on a Windows test VM.
- [ ] Verify service binary/configuration ACLs and QGA pipe access under the
    selected least-privilege account.
- [ ] Verify upgrade, rollback, and removal leave no stale service process or
    configuration behind.

**Gate:** Recovery does not trigger for intentional stop and does not create a tight restart loop.

## Phase 2 validation — live KVM

### V1. Guest Agent probe

- [!] Requires a running RHEL/libvirt host and Windows KVM guest.
- [ ] Run host prerequisite checks.
- [ ] Confirm the virtio-serial channel name `org.qemu.guest_agent.0`.
- [ ] Confirm QGA service availability and permissions in Windows.
- [ ] Run `guest-info` and `guest-get-memory-stats` at least three times.
- [ ] Record QEMU, libvirt, QGA versions, latency, and observed response fields.
- [ ] Capture the actual Windows pipe path and account/ACL behavior.
- [ ] Repeat the probe after QGA restart and guest reboot.

### V2. Live virtio-mem inspection

- [ ] Capture the virtio-mem alias and block size from live XML.
- [ ] Capture `requested`, `current`, and `size` values.
- [ ] Confirm the block size is compatible with the host configuration and THP assumptions.
- [ ] Check whether `dynamic-memslots=on` and `unplugged-inaccessible=on` are in use.
- [ ] Select a reversible, aligned target within configured limits.
- [ ] Confirm no update is issued while `requested != current`.
- [ ] Confirm the chosen VM, host, and workload do not rely on incompatible virtio-mem features.

### V3. End-to-end resize

- [ ] Perform one manual reversible live resize.
- [ ] Confirm convergence before a second request.
- [ ] Test QGA interruption, guest reboot, failed update, and service restart.
- [ ] Preserve evidence and update API/issue documentation with observed behavior.
- [ ] Verify host and guest logs can correlate one policy decision to one host
    request and one convergence result.

**Live validation gate:** No automatic memory updates until V1 and V2 pass.

## Phase 3 — Hardening

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

**Phase 3 gate:** Failures are actionable, observable, bounded, and covered by local tests.

## Phase 4 — Operations

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
              └──────────────→ M8 → M9 ─┐
                                        └→ M10 → M11 → M12
```

The live KVM path (`M8`) is external to the Windows build path, but M10 cannot
pass until both paths succeed.

## Active blockers and decisions

| ID | Blocker or decision | Impact | Owner/action |
| --- | --- | --- | --- |
| B1 | No continuously attached RHEL/libvirt host and Windows KVM guest in the local validation environment | Blocks M8–M10 live evidence | Run `scripts/check-environment.sh` and `scripts/validate-guest-agent.sh` on the KVM host |
| B2 | Actual Windows QGA pipe path and LocalService permissions are not yet verified | Blocks safe installation defaults | Confirm channel/device mapping on the guest before installation |
| B3 | SCM adapter and callback API are not implemented | Blocks install/start/stop/remove validation | Implement M5 against the portable `ServiceHost` and `StopSignal` boundaries |
| B4 | Persistent configuration location and format are not selected | Blocks production startup configuration | Choose a Windows-safe, least-privilege configuration mechanism in H3 |
| B5 | Concrete guest state and resize sinks are not wired | Blocks real automatic resize behavior | Implement M6 without invoking Linux commands from the guest |
| B6 | Event-log and recovery policy are not implemented | Blocks operational failure recovery | Implement H1/H2 and verify intentional versus unexpected exits |
| B7 | QGA, controller, libvirt, and `virsh` memory-unit semantics are not reconciled in one tested contract | Blocks safe resize enablement | Resolve in F6a before M9/M10 |
| B8 | Named-pipe I/O currently has no enforced operation deadline | A stuck QGA call can violate bounded shutdown | Implement cancellable/deadline-aware transport in F4/F5 |
| B9 | Shutdown timeout is configured but not yet enforced by the worker host | Stop-pending behavior cannot be proven | Add bounded join/worker termination policy in M3/M5 |
| B10 | No deterministic failure-injection harness exists | Live-only failures would be slow and difficult to reproduce | Implement F8a before M10 |
| B11 | Official virtio-mem guidance shows compatibility and safety limits that are not yet codified in the host contract | The controller can make unsafe assumptions about resize behavior or valid host configurations | Add the QEMU/libvirt compatibility gate and explicit validation checks in M9a before live automation |
| B12 | Contemporary virtio-mem guidance recommends `dynamic-memslots=on` with `unplugged-inaccessible=on` for safe unplugged memory handling | The host may misread unplugged-memory semantics without this configuration | Document and verify the host/guest configuration assumptions during V2 and M9a |

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
9. Documentation, recovery procedures, health checks, release evidence, and known limitations are current.

## Known risks

- QEMU Guest Agent availability and Windows virtio-serial permissions.
- Correct named-pipe path and access under the selected service account.
- Memory allocation hysteresis tuning under real workload pressure.
- Slow or interrupted QGA responses during bounded shutdown.
- SCM callback timing and recovery semantics.
- Cross-platform integration between the Windows guest and Linux KVM host.
