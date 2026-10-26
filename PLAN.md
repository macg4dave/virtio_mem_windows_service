# virtio-mem Windows service — Plan

This file replaces all previous docs (readme, BACKLOG, PROJECT_STATUS,
IMPLEMENTATION_PLAN, `docs/*`, and the `.github/prompts/*` set). They described
a 17-milestone roadmap and ~40 QA gates. Very little of that had run against a real VM.
Git history still has them if anything needs recovering.

## What this project is for

Give a Windows KVM guest more RAM when it needs it, and take RAM back when it
doesn't, by resizing a virtio-mem device from the host:

```text
Windows guest                         RHEL/libvirt host
-------------                         -----------------
service samples memory  --(file)-->   daemon reads it over QGA guest-file-read
(available, commit)                   computes a target size
                                      virsh update-memory-device --requested-size
                                      waits for live `current` to converge
```

The idea is simple and the mechanism is standard (libvirt
`update-memory-device`, QGA file reads). The implementation is not simple.

## Honest current state (2026-10-26)

| Item | Reality |
| --- | --- |
| Code size | 29,641 Rust source lines, including tests, comments, and blanks; not a production-code count |
| `virtio-mem-core` + `host` tests | Rerun on 2026-10-26: 96 + 85 pass with `--locked` |
| `tools/xtask` | Initial E0063 reproduced, then fixed by completing the existing grow-only analysis and rejecting lower requested targets; local tests pass |
| `windows` crate | Requires a Windows target. Cross-compilation from Linux needs the appropriate toolchain; native service behavior requires Windows execution |
| Automatic resizing | Earlier "never enabled" claim was wrong. Archived workload run `qualification-1789231813766-72447` records 1,098,907,648 bytes of peak growth but zero accepted reclaim; the later passing run explicitly required no resizing. The installed host unit is currently disabled/inactive |
| Multi-VM pool (`global_pool.rs`, `pool_ledger.rs`, ~2.4k lines) | Internal/test callers and public exports, but no host integration found in this workspace; external consumers have not been established |
| `behavior_evidence.rs` (~500 lines) | Only reachable from a host CLI subcommand |
| Host config | ~45 `VIRTIO_MEM_*` env vars, 2 demand-source modes, 2 raw transports, and shadow/growth/legacy pressure-policy modes |
| Shrink rollout | Default and example now disable automatic shrink; explicit opt-in remains for separately qualified deployments |

Growth has occurred in an earlier automatic workload run. That does not prove
reliable shrink or unattended operation. Those outcomes still require direct
evidence; a passing shadow/no-resize run is not a resizing qualification.

## Facts worth keeping

- Virtio-mem is asynchronous: `requested` is the target, `current` is what the
  guest actually plugged. A successful `virsh` call doesn't mean it happened.
  Always poll live XML for `current`.
- Windows virtio-mem driver is tech preview. **Unplug (shrink) can be partial,
  slow, or stall.** Grow is the safe direction.
- Libvirt `dommemstat actual` is balloon state, not virtio-mem allocation.
  `unused`/`available` from the balloon stats can serve as demand hints.
- Upstream QGA has no `guest-get-memory-stats`; the custom-command path in
  `host/src/qga.rs` won't work on a stock guest agent.
- Windows has no "recommended RAM" API. Usable signals: available physical,
  commit total vs. commit limit, and the low/high memory resource
  notifications.
- Targets must be aligned to the device block size and kept within
  `[min, max]` of the device. Never request the full device size blindly.
- Never send a new resize while the previous one hasn't converged. If it
  stalls, stop and log; don't retry in a loop.
- Leave the host enough RAM (check `/proc/meminfo` MemAvailable before growing).

## Plan

### Phase 0 — Initial platform investigation (interrupted)

The static assessment is complete. The separately approved bounded live check
below was interrupted after repeated authentication prompts. The user explicitly
redirected the work: the program must own the operation, rather than the agent
running a manual loop. Local implementation now proceeds in Phase 1; repeated
grow/shrink and workload qualification are not claimed complete. No more manual
live-VM commands are authorized by the interrupted test.

1. On the test VM, run `virsh update-memory-device VM --alias ALIAS --requested-size X`
   to grow, then to shrink, several times. Watch `virsh dumpxml` `current` and
   Windows Task Manager.
2. Record: does grow work every time? How long does it take? Does shrink
   complete, partially complete, or hang? Under memory load?
3. Check whether `virsh dommemstat VM` returns useful `unused`/`available` on
   Windows with the balloon driver + stats period set. If yes, **we may not
   need a Windows service at all.**

Before live work, identify the exact VM, device alias, baseline requested/current
allocation, block size, bounded targets, host reserve, and rollback or recovery
procedure. State target units explicitly. Observe convergence before issuing
another resize. A stalled operation may prevent ordinary rollback; do not assume
that reversing a target is safe or accepted. Reboots and VM definition changes
require approval.

**Exit:** record repeated grow/shrink outcomes, convergence times, partial or
stalled unplug, behavior under load, and dommemstat usefulness here. Select the
telemetry source and decide whether automatic shrink is viable. If shrink is
unreliable, the product is grow-only with an explicitly reviewed manual or
reboot-time reset; shrink waits until the platform supports it.

### Phase 0 operational evidence (2026-10-26)

- Test VM: `win11_gpu`, device `ua-virtiomem0`, running. Controller unit
  `virtio-mem-host@win11_gpu.service` is disabled/inactive with `Restart=no`.
- Live device size: 21,474,836,480 bytes (20 GiB); block size: 2,097,152 bytes
  (2 MiB). Baseline requested/current: 1,073,741,824 bytes (1 GiB).
- QGA ping succeeds. The existing Windows service publishes fresh v3 telemetry
  at `C:\ProgramData\VirtioMemService\telemetry.json`, with about 4.4 GB
  physical RAM available and a high-memory notification during inspection.
- `virsh dommemstat` returns advancing five-second statistics, including
  `unused`, `available`, and `usable`. Baseline `available` was 9,367,852 KiB;
  it increased to 9,433,388 KiB after the 64 MiB grow. These are useful host-side
  demand inputs; `actual` remained balloon state and is not allocation authority.
- QMP confirms dynamic memslots and unplugged-inaccessible. One initial
  read-only QMP call timed out; a subsequent read succeeded.
- SSH alias `virtio-mem-windows` currently resolves to `ice101.lan` and times out.
  The earlier guest workload executable path is also absent. No workload,
  service restart, deployment, reboot, or VM-definition change was performed.
- Local checks: `cargo test -p virtio-mem-core -p virtio-mem-host --locked`
  passed (181 tests). Adding xtask fails with E0063 as recorded above. Only
  the Linux Rust target is installed; no current Windows binary build is claimed.
- Explicit user approval covered at most three cycles from 1 GiB to
  1,140,850,688 bytes (1 GiB + 64 MiB; 1,114,112 KiB to virsh) and back to
  1 GiB (1,048,576 KiB). Require 4 GiB host reserve; allow 120 seconds for growth
  and 300 seconds for restoration. Stop on the first stall/uncertain outcome
  without another resize. No other live mutations are included.
- First growth converged in approximately 4.297 seconds. Restoration reclaimed
  only 10 MiB before remaining unchanged through the last successful observation
  at about 225.739 seconds. Polling then failed because authentication was
  cancelled. This was an interrupted observation, not a completed 300-second
  timeout or a successful restoration. No second cycle or recovery was issued.
- Last observed device state: requested 1,073,741,824 bytes (1 GiB), current
  1,130,364,928 bytes (1 GiB + 54 MiB). The baseline has **not** been confirmed
  restored. No later live-state check was attempted after the user's correction.
- The manual loop opened a fresh read-write libvirt connection for every query,
  triggering repeated desktop authorization. The user reported more than fifty
  password prompts. The loop has exited; no polling/virsh child remains. Future
  validation must use the program with service authorization configured once.

### Phase 1 — Cut to a minimum working loop

**Current phase: local program implementation, following the user's explicit
redirection.** No deployment, permission change, service start/restart, recovery,
or additional live resize has been performed. The interrupted platform checks
remain acceptance work, to be exercised through the application.

Keep:

- Windows service: sample `GlobalMemoryStatusEx` and commit info every N
  seconds, write one JSON line atomically. Install/run as a service. (Skip
  this if Phase 0 says dommemstat is enough.)
- Host daemon, one VM, one device:
  1. read guest stats (QGA file read or dommemstat)
  2. read live device XML (`requested`, `current`, block size, max)
  3. compute one bounded, block-aligned device target using the contract below
  4. grow promptly in bounded steps when target exceeds current; shrink only after
     sustained low usage (hysteresis + cooldown), in small steps
  5. one command in flight; wait for `current == requested` with a timeout;
     on timeout, stop resizing and log loudly
  6. refuse to grow if host MemAvailable is below a reserve
- One small host config file (TOML proposed): vm, alias, device min/max,
  headroom, step, poll interval, shrink cooldown, host reserve, enable_shrink,
  and necessary transport/timeout inputs. Shrink defaults off. Reduce the
  existing modes with the migration; do not add TOML as a parallel config path.
- Unit tests for the target math and the XML parsing.

The earlier shorthand `target = clamp(commit_used + headroom, min, max)` is not
yet an implementation specification. Guest-wide commit bytes and device bytes
have different meanings. A 10 GiB total-RAM requirement with 8 GiB fixed base RAM
implies a 2 GiB device contribution, before device bounds and alignment. Define
the chosen telemetry's meaning, physical safety floor, fixed-base mapping,
alignment, effective maximum, overflow behavior, and missing/stale-input behavior
before replacing the existing math. Dommemstat does not provide the same inputs
as Windows commit telemetry. Start from the existing checked calculations in
`core::target_controller`; selecting a different formula is an intentional
policy change, not an equivalent refactor.

Proposed removals, subject to the Phase 0 result and retained contracts:

- `global_pool` and `pool_ledger` (no host integration found); `behavior_evidence`
  is CLI-reachable, so its removal is deliberate feature retirement.
- `pressure_assessment` and the shadow/growth/legacy policy modes in
  `host/src/target_policy.rs`; replace them with the single specified policy.
- `shrink_recovery` until shrink is proven, compatibility attestation,
  controller-status v3, the custom QGA memory-command path, and
  the unused parts of `core::demand` (e.g. `DemandCalculator`).
- Most of `tools/xtask`: qualification, deployment, host_deploy, calibration,
  preflight. Keep at most a `cargo xtask windows-build` cross-compile helper.
  Direct Cargo commands, listed below, are the local gate.

Do not remove dommemstat with the unsupported custom QGA command if Phase 0
selects dommemstat. Preserve command ownership, uncertain-outcome handling,
host reserve checks, and required recovery behavior when removing larger
compatibility/status/recovery layers. Check actual API and CLI consumers before
removing public contracts; do not introduce speculative compatibility layers.

**Exit:** one understandable loop, no superseded implementation paths, passing
non-Windows tests, and a Windows binary build for the selected target
(`x86_64-pc-windows-gnu` or `-msvc`) if the service remains necessary. Native
service behavior still needs separate Windows execution. A few thousand lines
is a simplification aim, not a measured reduction or correctness criterion.

### Phase 1 implementation progress (2026-10-26)

- The host daemon checks `org.libvirt.unix.manage` authorization before its
  loop, using `pkcheck` without user interaction and the exact process
  PID/start-time/UID. Missing access becomes an explicit startup failure.
- The shared virsh adapter uses read-only connections for XML, dommemstat,
  domain-state, and version queries. It checks local management authorization
  before commands requiring a write-capable connection. This targets the local
  RHEL `qemu:///system` deployment; remote transports retain their own contract.
- The existing host deployment command now includes a reviewable polkit rule
  for the dedicated `virtio-mem-host` account in its backup/install/evidence
  workflow. This is connection-level libvirt management access, **not** a
  per-VM ACL. It grants no new rights to the desktop user. The rule is prepared
  in the repository and has **not** been installed on the host. Administrative
  installation is a one-time setup step; the daemon does not elevate itself or
  store a password. Local system access assumes the deployment uses polkit.
- The shared command runner drains stdout/stderr while the child runs, enforces
  deadlines, and reaps timed-out children. Tests use a local Rust subprocess
  fixture and never contact libvirt or polkit.
- Owned durable growth now persists the actuation latch and returns a convergence
  timeout when its deadline expires, retaining the unresolved intent. It does
  not merely change a log label and keep polling forever.
- Invalid saved command intent now blocks checkpoint loading rather than
  silently discarding a possible latch and starting cold. Regression tests for
  both this case and growth timeout were observed failing before the fixes.
- Automatic Windows shrink defaults off, and the repository's systemd config
  example matches. A runtime regression test verifies that a lower calculated
  target remains advisory under the default setting. The old default-on config
  expectation was updated because the rollout behavior intentionally changed.
- Xtask's missing analysis field now counts lower requests; its existing
  grow-only qualification mode rejects even the first shrink from a converged
  allocation. The parser already exposed this mode, but its invariant was not
  enforced. The CLI help now describes it.
- Host and xtask tests pass locally (95 + 45). Final workspace/lint/build results
  are recorded after verification. No current Windows build or live deployment
  of these changes is claimed. The broader simplification register remains open.

Authorization behavior follows the upstream [libvirt authentication contract](https://libvirt.org/auth.html)
and [`pkcheck` non-interactive/process-identity requirements](https://polkit.pages.freedesktop.org/polkit/pkcheck.1.html).
The non-interactive check is a prerequisite, not a replacement for libvirt's
own authorization. Host policy and native behavior still require deployment
verification through the program, without the previous manual command loop.

### Phase 2 — Run it for real, grow-only

- Deploy to the test VM with `enable_shrink = false`.
- Run memory-hungry workloads in the guest. Confirm it grows in time and the
  guest doesn't page heavily or OOM.
- Run for a few days. Watch the logs for stalls.

**Exit:** grow works unattended.

### Phase 3 — Shrink

- Turn on shrink with conservative cooldowns and small steps.
- Verify partial/stalled unplug is detected and the loop stops cleanly
  instead of fighting the guest.
- Only then consider Windows memory-resource notifications as a faster
  "grow now" trigger.

**Exit:** a week of grow and shrink on the test VM without manual intervention.

### Later, only if needed

- Multiple VMs sharing a host RAM budget with priorities. If the current pool
  code is removed, git history retains it. Build host integration only after
  Phases 2–3 hold on one VM.
- Linux guest support (virtio-mem in Linux is mature; this would be easier
  than Windows).

## Architecture assessment (static review, 2026-10-26)

The project needs fewer competing control paths and one owner of resize state.
The existing crate boundaries are useful; the main complexity sits inside them.
This assessment inspected workspace structure, dependencies, public interfaces,
entry points, significant workflows, state/persistence paths, and current
instructions. It is not evidence that the platform works.

The review changed no source files and ran no builds, tests, remote commands,
or VM actions. Existing staged documentation consolidation was preserved. The
assessment was recorded without implementing the proposed refactors. The
later adoption of engineering instructions is recorded below.

### Workspace and execution paths

| Crate | Actual responsibility | Main dependencies |
| --- | --- | --- |
| `virtio-mem-core` | Telemetry contracts, XML parsing, target estimation, reconciliation, pressure policy, pool accounting | Serde, quick-xml, thiserror, SHA-256 |
| `virtio-mem-host` | Configuration, telemetry acquisition, policy persistence, host-memory checks, virsh execution, diagnostics | Core, signal-hook, wait-timeout |
| `virtio-mem-service` | Windows sampling, atomic telemetry publication, SCM lifecycle, workload utility | Core, winapi, Serde |
| `virtio-mem-xtask` | Local gates, remote builds, deployment, qualification, live-resize orchestration | Core, host, subprocess utilities |

- **Windows service:** command dispatch -> configuration -> `RawTelemetryWorker`
  -> native memory APIs -> telemetry envelope -> `AtomicRawTelemetryPublisher`.
  SCM and console startup construct the worker separately.
- **Host raw telemetry:** configuration -> file or QGA file reader ->
  validation/replay acknowledgement -> `TargetDemandSource` -> estimator and
  optional pressure policy -> `HostRuntime` -> host-memory gate -> resize sink
  -> live XML observation.
- **Host guest stats:** dommemstat or custom QGA command -> `plan_resize` -> the
  same runtime, with different persistence and recovery behavior.
- **Operational tooling:** xtask dispatches deployment and qualification,
  invokes local/remote processes, writes evidence, and can start controllers
  or workloads. It is not merely a build-command wrapper.

Useful existing foundations are alias-selected XML, checked unit conversions,
explicit requested/current observations, atomic telemetry publication, and the
distinction between command rejection and an unknown command outcome.

The largest weaknesses are distributed resize/lifecycle rules, diagnostics that
can describe a different policy from the daemon, separate persistence paths for
sample acceptance and commands, experimental configuration combinations,
privileged tooling that has grown into another application, and public
implementations with no production callers found in this workspace.

## Code consolidation register

Source references describe the reviewed snapshot and may move during refactoring.
"No production caller" means none found in this repository. Public exports alone
do not establish whether external consumers exist. All executable changes below
belong to future work after the current phase's exit criteria.

### 1. COMBINE — Resize lifecycle and authoritative command state

- **Evidence/callers:** `host/src/runtime.rs:249`, `HostRuntime::run`;
  `host/src/target_policy.rs:63`, `TargetDemandSource`;
  `crates/virtio-mem-core/src/reconciler.rs:78`, `reconcile`;
  `crates/virtio-mem-core/src/shrink_recovery.rs:142`, `ShrinkOperation`.
  Each represents parts of pending operations, ownership, latching, or
  convergence. Runtime keeps local ownership flags alongside checkpointed
  intent. Durable pending shrink takes an earlier branch than the local
  shrink observer.
- **Canonical direction:** host runtime owns operation state, a pure reconciler
  chooses actions, persistence stores that state, and status derives from it.
- **Benefit:** remove competing lifecycle interpretations and synchronized flags.
- **Preservation risk:** retain restart ownership, uncertain outcomes, and required
  recovery before removing either path.

### 2. IMPROVE — Preserve the stop condition on invalid checkpoints

- **Evidence/callers:** `host/src/target_policy.rs:1026`,
  `load_matching_checkpoint`, used during controller construction and status
  loading, returns `Ok(None)` for invalid intent identity or fingerprints.
  It discards the whole checkpoint, potentially including an actuation latch.
  Other malformed-checkpoint branches correctly refuse startup.
- **Canonical behavior:** consistently reject invalid control state. Only
  expendable estimator history may become cold.
- **Benefit:** corrupt intent cannot silently become permission to actuate.
- **Verification/risk:** cover malformed intent with a persisted latch and
  unresolved commands; do not reset ownership as a recovery shortcut.

### 3. COMBINE — Convergence timeout behavior

- **Evidence/callers:** `host/src/runtime.rs:342` keeps observing durable growth
  after timeout and changes its log label to `constrained`. The pending-request
  branch at line 580 returns `ConvergenceTimeout`; durable shrink latches through
  another branch. These paths serve the same runtime with different sources.
- **Canonical behavior:** one documented timeout transition consistent with
  stopping actuation and reporting the stall.
- **Benefit:** source selection does not change the timeout contract.
- **Verification/risk:** retain observation and prohibit replay; cover both
  telemetry sources and restart, including how deadlines behave across restart.

### 4. COMBINE — Daemon and CLI policy evaluation

- **Evidence/callers:** `host/src/cli.rs:551`, `run_configured_decision`, selects
  only `stats_source` and invokes the threshold policy. It ignores the raw
  demand source and pressure policy selected by `host/src/main.rs:59`.
- **Canonical direction:** share the selected pure policy evaluation, or retain
  the command as an explicitly named guest-stats diagnostic.
- **Benefit:** diagnostic output describes the actual configured policy.
- **Verification/risk:** the existing raw evaluator writes acknowledgements and
  checkpoints; do not reuse it directly for a read-only preview. Verify decision
  parity for identical inputs and absence of writes.

### 5. IMPROVE — Specify target semantics before replacing math

- **Evidence/callers:** `crates/virtio-mem-core/src/target_controller.rs:375`,
  `calculate_instantaneous`, accounts for physical usage, commit headroom,
  fixed visible base RAM, alignment, and effective device maximum. The host
  raw evaluator uses it through `TargetEstimator`. At
  `crates/virtio-mem-core/src/pressure_assessment.rs:440`,
  `assess_windows_pressure` computes a different commit-based requirement.
- **Canonical starting point:** existing checked math in `target_controller.rs`.
  These are alternative policies, not interchangeable helpers. The replacement
  contract is described in Phase 1 above.
- **Benefit:** simplify without confusing total RAM and device allocation.
- **Verification/risk:** preserve physical floors, capacity limits, alignment,
  and overflow handling. A different policy is an intentional behavior change.

### 6. COMBINE — Accepted-sample identity and persistence

- **Evidence/callers:** `host/src/raw_telemetry.rs:290`, `validate_contents`,
  serves file and QGA readers and persists replay acceptance before the
  estimator consumes the sample. `TargetEstimator::prepare_sample` at
  `crates/virtio-mem-core/src/target_controller.rs:336` independently tracks
  ordering; `PressureHistoryState::observe` does so again.
- **Canonical direction:** transport obtains bytes, envelope validation
  establishes validity, and host policy state owns acceptance. Retain distinct
  history-window checks where their purposes differ.
- **Benefit:** fewer competing acceptance records and persistence-order problems.
- **Verification/risk:** preserve unchanged-record handling, altered-record
  rejection, retired sessions, and restart continuity. Persistence failure must
  not become successful acceptance on the next read.

### 7. IMPROVE / DELETE — Configuration and policy modes

- **Evidence/callers:** `host/src/config.rs:142`, `HostConfig::from_env`, loads
  settings for multiple paths regardless of selection. There are two demand
  modes, two raw transports, and three pressure modes. At line 8,
  `DEFAULT_AUTOMATIC_WINDOWS_SHRINK` is true; the example config also enables it.
- **Canonical direction:** after Phase 0, one selected telemetry path and one
  policy through `HostConfig`; default shrink off for the planned rollout.
- **Benefit:** fewer invalid combinations and irrelevant required settings.
- **Verification/risk:** migrate configuration with mode removal, including the
  systemd example. Introduce TOML only with that migration; preserve required
  transport settings, timeouts, permissions, and deliberate defaults.

### 8. DELETE — Pool accounting without host integration

- **Evidence/callers:** `crates/virtio-mem-core/src/global_pool.rs:300`,
  `arbitrate_host_pool`, and `pool_ledger.rs:635`, `FilePoolLedgerStore`, have
  internal/test callers and public exports but no host integration found.
  The ledger also puts filesystem persistence inside the policy crate.
- **Retained authority:** the single-device host controller. Remove these two
  modules and their exports in Phase 1 after consumer checks.
- **Benefit:** eliminate unintegrated multi-VM policy, accounting, and persistence.
  Core's direct `sha2` dependency then becomes removable; host still uses it.
- **Preservation risk:** public API removal may affect external consumers; do
  not claim that exports prove either use or non-use outside the workspace.

### 9. DELETE — Superseded implementations with test/export uses only

- **Evidence/callers:** `DemandCalculator` in
  `crates/virtio-mem-core/src/demand.rs:621`; `NativeTelemetryWorker` in
  `windows/src/runtime.rs:12`; `JsonLinesRawTelemetryPublisher` in
  `windows/src/demand.rs:31`; `ScmHandler` in `windows/src/service_scm.rs:99`.
  Workspace searches found tests/exports but no production callers for these.
- **Canonical implementations:** host-owned target policy, `RawTelemetryWorker`,
  `AtomicRawTelemetryPublisher`, and the actual SCM callback/context.
- **Benefit:** remove alternate demand, sampling, publication, and lifecycle paths.
- **Verification/risk:** remove associated exports and obsolete error variants
  after consumer checks. Move useful behavioral tests to retained paths rather
  than preserving unused code to satisfy its tests.

### 10. COMBINE / DELETE — Compatibility discovery and resize preparation

- **Evidence/callers:** `host/src/compatibility_source.rs:62`,
  `VirshQmpCompatibilitySource::property`, overlaps the production attestation
  collector's QOM queries and boolean parsing; its uses are tests. Four
  preparation methods in `host/src/resize_sink.rs:102` repeat XML acquisition,
  compatibility validation, conversion, and command construction. Runtime and
  manual CLI operations use the sink.
- **Canonical direction:** retain the currently wired attested collector while
  needed, remove its unused alternative, and share preparation mechanics only
  among operations that survive Phase 1.
- **Benefit:** fewer discovery and command-construction implementations.
- **Preservation risk:** ordinary resize, renotification, supersession, and
  recovery have different preconditions. Do not merge those checks or remove
  the immediate live-state race check as apparent duplicate work.

### 11. COMBINE / IMPROVE — Windows startup and cancellation

- **Evidence/callers:** worker construction is repeated in `run_service` at
  `windows/src/main.rs:58` and `service_main` at
  `windows/src/service_scm.rs:402`. Separately, `StopSignal` at
  `windows/src/service_host.rs:54` updates cancellation and notifies without
  holding the mutex used by condition-variable waits. Cancellation can occur
  between a predicate check and waiting, losing the notification.
- **Canonical direction:** one worker-construction path and one correctly
  synchronized cancellation protocol in the existing service lifecycle.
- **Benefit:** console and SCM execution cannot drift; stopping wakes waiters.
- **Verification/risk:** preserve dispatcher attachment before configuration
  loading and correct Running/Failed publication. Use controlled interleavings
  for cancellation during initialization, polling, and shutdown.

### 12. DELETE / IMPROVE — Tooling duplicates product contracts

- **Evidence/callers:** `tools/xtask/src/windows.rs:765`, `windows::deploy`,
  independently constructs Windows config JSON with hardcoded schema version
  4. Qualification, deployment, preflight, and controller-status commands add
  further state, validation, process, and evidence layers, dispatched by xtask.
- **Canonical direction:** follow Phase 1's tooling reduction rather than build
  a shared tooling framework. If deployment remains, use the service's
  authoritative serialization contract.
- **Benefit:** fewer independently evolving schemas and operational workflows.
- **Preservation risk:** remove commands deliberately with their users. In
  particular, `behavior_evidence` is CLI-reachable; retirement changes a feature
  contract and must not be described as dead-code deletion.

### 13. IMPROVE — Process execution and failure reporting

- **Evidence/callers:** `host/src/virsh.rs:57`, `Virsh::run`, waits before
  draining piped output; enough output can block the child and produce a
  misleading timeout. All host virsh adapters use it. Xtask's `bounded_output`
  in `tools/xtask/src/process.rs:75` drains concurrently but duplicates much
  of `bounded_output_with_env`.
- **Canonical direction:** improve the existing host adapter using concurrent
  draining; consolidate surviving tooling locally.
- **Benefit:** command completion and timeout reports reflect execution rather
  than pipe backpressure; preserve contextual causes and rejected/unknown
  command distinctions through existing error boundaries.
- **Verification/risk:** exercise large stdout/stderr, failure, timeout, and
  child cleanup using local subprocess fixtures when tests are authorized.
  Do not create a workspace process framework for tooling scheduled for deletion.

### 14. KEEP / SEPARATE — Different transport and platform contracts

- **Evidence/callers:** `host/src/qga.rs` and `host/src/dommemstat.rs` provide
  different evidence. `windows/src/demand.rs` uses Windows atomic replacement;
  `host/src/target_policy.rs` publishes durable Linux checkpoints. Windows Event
  Log and host stderr/journal output also have different consumers.
- **Canonical shared foundations:** `parse_virtio_mem_xml_for_alias` at
  `crates/virtio-mem-core/src/virtio_mem_xml.rs:181`, `VirtioMemState`, and checked
  byte/KiB conversions. Keep platform acquisition and publication separate.
- **Benefit:** reuse actual shared rules without hiding important OS semantics.
- **Preservation risk:** a fresh read immediately before a command is justified
  even if an earlier poll read the same resource. Do not remove it as duplication.

### Tooling limitations affecting verification

- `QualificationAnalysis` declares `lower_request_violations`, but `analyze`
  at `tools/xtask/src/qualification.rs:1392` omits it from construction. This
  is a source-visible compile blocker; the compiler was not run in this review.
- `live_resize::run` at `tools/xtask/src/live_resize.rs:208` attempts ordinary
  rollback after forward failure. Ordinary resize preparation rejects an
  unconverged device, so rollback cannot be assumed to recover a stalled
  operation. Preserve the no-stacked-resizes rule and specify operator recovery.

## Engineering standards and instruction adoption

### Responsibility ownership

| Responsibility | Authoritative home |
| --- | --- |
| Wire telemetry schema and intrinsic validation | Core telemetry types |
| Device geometry and XML interpretation | Core device/XML code |
| Target calculation and pure action selection | Core policy |
| Accepted samples, pending operation, timeout, latch | Host controller state |
| External commands and host-resource checks | Host adapters |
| Windows sampling, publication, service lifecycle | Windows crate |
| Presentation | CLI, journal output, Windows Event Log |
| Project evidence and phase acceptance | `PLAN.md` |

Use one implementation per responsibility and preserve meaningful platform
differences. These are the proposed target boundaries; the current code does
not yet consistently follow them.

### Instruction adoption (2026-10-26)

The reviewed instruction proposal and the supplied Rust guidance have been
adapted and adopted in [`.github/copilot-instructions.md`](.github/copilot-instructions.md).
`AGENTS.md` continues to point there. The former draft contents and Working
rules are not repeated here; the instruction file is the single authoritative
home for engineering, review, consolidation, testing, and live-operation rules.

The adaptation follows this workspace's actual responsibilities:

- Memory units, fixed-base/device mapping, telemetry acceptance, pending
  operations, and asynchronous convergence replace the source project's device
  model and UI concerns. Existing typed errors, CLI exits, journal output, and
  Windows Event Log remain the platform conventions.
- Required Win32/host FFI remains permitted with scoped safety contracts; a
  blanket unsafe ban or generated Slint-code exception would not fit this code.
- Ownership follows runtimes, threads, signals, and SCM callbacks. There is no
  blanket requirement for single-threaded `Rc<RefCell<_>>`, identifier wrappers,
  new modules, or arbitrary function/file-size limits.
- The existing local gate uses formatting checks and Clippy with warnings
  denied. Manifest inspection found no configured pedantic/restriction groups
  or numeric caps. No lint configuration was changed or new enforcement claimed.
  Existing lint suppressions and runtime panic shortcuts remain cleanup debt
  for appropriately scoped code changes, not precedent for new ones.
- Tests target memory math, XML/telemetry parsing, persistence, cancellation,
  and resize failure/recovery behavior. Slint, CUDA, model fixtures, GPU-only
  inference tests, and translation-file requirements do not apply.
- Tests serve required behavior: obsolete tests need an independent reason for
  change; neither weakening tests nor distorting the design to pass them is
  acceptable.

Only instruction/documentation files changed for this adoption. No executable
source, Cargo manifest, or dependency changed, and no builds, tests, remote
commands, or VM actions were run. Phase 0 remains the current phase. The
architecture ownership table is the target design; adoption of the standards
does not claim that existing code already meets it.

## Refactoring execution order

The initial manual platform check was interrupted and the user redirected work
to the application. Phase 1 now proceeds locally; platform qualification remains
required before claiming unattended operation. Keep each change contained and
independently reviewable/revertible; do not turn this register into a parallel
milestone system.

| Order | Task | Acceptance and verification |
| --- | --- | --- |
| 0 | Complete the existing platform investigation | Record repeated grow/shrink outcomes, timing, stalls, and dommemstat usefulness here. Separately authorized live work; no implementation expansion. |
| 1 | Define the minimum loop's contract | Specify telemetry source, whole-guest/device mapping, physical floor, aligned maximum, default-off shrink, timeout, restart ownership, and recovery before replacing policy. |
| 2 | Establish a runnable local baseline | Repair the qualification initializer if xtask remains. Run existing non-Windows tests once implementation work is authorized; record actual results. |
| 3 | Correct failure-state handling | Invalid pending checkpoints cannot clear a latch; timeout behavior is consistent; cancellation cannot miss a wakeup. Verify malformed checkpoints, fake observations/clocks, and controlled thread ordering. |
| 4 | Remove isolated unused implementations | Remove pool modules and test/export-only alternatives after consumer checks. Remove associated exports/dependencies; retained tests pass and reference searches find no superseded callers. |
| 5 | Unify action selection and lifecycle state | One pending-operation owner and one action path serve the retained source. Cover convergence, external divergence, rejection, ambiguous execution, timeout, cancellation, and restart. |
| 6 | Make diagnostics reflect the selected policy | Preview agrees with daemon policy for identical inputs and writes no controller or replay state. |
| 7 | Consolidate sample acceptance and persistence | One authoritative accepted identity; persistence failure cannot masquerade as acceptance. Cover unchanged, replayed, altered, stale, restarted, and retired-session records. |
| 8 | Reduce modes, configuration, and tooling | Remove settings and feature paths together. Update Cargo membership, aliases, Make targets, and IDE tasks to reference retained commands. |
| 9 | Simplify Windows wiring and finish verification | One worker-construction path; retained atomic publication and SCM tests pass. Build for a Windows target, then separately verify native service behavior. |

For authorized implementation changes, the Linux regression gate while xtask
remains is:

```bash
cargo test -p virtio-mem-core -p virtio-mem-host -p virtio-mem-xtask --locked
```

Run focused checks first, then the non-Windows workspace gate:

```bash
cargo test --workspace --exclude virtio-mem-service --locked
```

Update the commands if xtask is removed. Windows compilation needs the chosen
target/toolchain and an actual binary build, not just `cargo check`. Native
service tests and VM behavior are separate verification steps. No builds or
tests were run to produce or record this assessment; do not interpret these
commands as completed checks or authorization for live tests.

## Expected simplification

The source evidence supports removing pool arbitration/persistence, unused
demand recommendation code, alternate Windows workers/publishers, the unused
SCM handler model, and the unused QMP compatibility adapter, subject to public
consumer checks.

After the Phase 0 decision, further reductions can remove alternative policy
modes, duplicate histories, recovery machinery the retained product does not
support, CLI-only evidence contracts, and most xtask deployment/qualification
infrastructure. Preserve required failure and recovery behavior before removing
larger layers. Core's direct SHA-256 dependency can go with pool persistence;
do not assume the host's SHA-256 dependency is unused while attestation and
fingerprints remain.

The intended result has three clear responsibilities: shared contracts and
pure policy, a host control loop, and a Windows telemetry service only if needed.
Exact code reduction depends on the platform result and deliberately retained
behaviors. No code-reduction percentage or completed refactor is claimed.
