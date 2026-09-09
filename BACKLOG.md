# BACKLOG

## 2026-09-09 M10g live audit and controller hardening

- The current checkout passes 62 shared-core and 67 host tests after adding
  regressions for unchanged atomic telemetry, transient invalid-demand
  observation, and fail-closed checkpoint control state. The native Windows
  gate passes 67 tests and produced executable SHA-256
  `607c2baed981e4807f082341aacb7d80ec846190c50d3b8f26728656aa55d899`.
- A bounded interactive run of that Windows candidate published two complete
  version-2 native snapshots 30 seconds apart with sequence `0` then `1`,
  monotonic time `0` then `30016`, correct VM/service identity, and retained
  the predecessor. Cleanup removed the task-created ProgramData directory.
- The operator-run candidate host batch observed the current host binary for
  90 seconds with `NRestarts=0`. It emitted exactly one fail-closed rejection
  for the known stale `domain_xml` attestation, issued no resize or unknown-
  command event, preserved QEMU/Windows/service continuity and
  `requested=current=2105344 KiB`, then restored the original binary/unit and
  removed its temporary configuration.
- The reader now distinguishes a fresh record from the expected unchanged
  atomic snapshot between producer writes. Unchanged input waits without
  policy advancement, history invalidation, or actuation. Invalid demand while
  converged is logged and observed instead of causing a systemd restart loop.
- Target-policy checkpoint writes now sync the file and containing directory.
  Malformed or oversized checkpoints fail closed, and identity/fingerprint
  mismatch cannot silently discard a durable latch or pending command intent.
- M10g remains in progress: the installed deployment still uses guest-stats
  compatibility mode, production raw transport/current Windows installation
  remain, and attestation must be refreshed before workload actuation.

## 2026-09-09 M10f desired/requested/current reconciliation

- Completed TASK-027 with a platform-neutral reconciler that keeps durable
  desired, device requested, authoritative current, and control health
  separate. Ordinary movement remains bounded to 1 GiB growth or 64 MiB
  reclaim, pending growth is only observed, and an owned pending shrink can
  only be superseded upward as far as current.
- The host now atomically records command intent before raw-telemetry
  actuation, immediately resolves command results against fresh live state,
  resumes observation without replay after restart, freezes an owned shrink
  when telemetry becomes invalid, and durably latches ambiguous, unapplied, or
  stalled operations.
- Added a dedicated resize-adapter precondition for upward pending-shrink
  supersession plus a `clear-latch` CLI that is dry-run by default, recollects
  compatibility evidence, refuses `requested != current`, and records the
  operator-visible reason when applied.
- Hermetic validation passes 62 shared-core and 65 host tests plus formatting
  and warnings-as-errors Clippy. No Windows files changed; the latest native
  Windows result remains 67 tests and was not rerun for TASK-027.

## 2026-09-09 M10e quantitative target estimator

- Implemented the host-owned absolute target estimator with checked physical-
  use and commit-headroom candidates, fixed-visible-base drift validation, the
  aligned effective maximum and explicit capacity-limited output.
- Added distinct normal and safe-floor reserves, immediate upward desired
  movement, bounded 10-minute high-water history, gap/session invalidation,
  256 MiB downward hysteresis, conservative restart warm-up, and the
  deterministic +4 GiB then +2 GiB target trace.
- Added required host configuration and a fingerprint-bound policy checkpoint
  under the systemd state directory. Missing or safely incompatible estimator
  state restarts reclaim warm-up; malformed/oversized state and fingerprint
  drift with pending control state fail closed. M10f owns command-journal,
  live control-health, and durable latch behavior.

## 2026-09-09 M10b operator-accepted closure

- Closed M10b/TASK-022 at operator direction using the completed hermetic
  interruption, restart, cancellation, ownership, stall, typed-failure, and
  no-replay matrix plus the installed-candidate rejection, bounded one-block
  recovery, and larger partial/no-progress live evidence.
- The clean active-controller reboot and refreshed-attestation batch was not
  completed and is not claimed as passing evidence. It remains an optional
  operational follow-up rather than a TASK-022 exit gate.
- ISSUE-015 is resolved by the accepted bounded recovery scope. M10f/M10g
  retain responsibility for durable desired/requested/current reconciliation
  and broader controller/platform qualification.

## 2026-09-09 M10b runtime recovery hardening

- Corrected the allocation-neutral M9d domain fingerprint to normalize
  libvirt's derived live `currentMemory` content as well as the selected
  virtio-mem `requested` and `current` contents. Similarly prefixed elements
  and all non-allocation configuration remain bound.
- Added explicit resize-sink error classes for preparation rejection versus an
  invoked update with unknown outcome. Owned-shrink observation failure,
  invalid state, external target change, re-notification failure, deadline,
  and cancellation now latch or stop without blind replay.
- Added operation-correlated request, progress, re-notification, convergence,
  ownership, stall, cancellation, and recovery-required journal events;
  unchanged unowned divergence is reported once rather than every poll.
- Deterministic cancellation/restart, state interruption, external ownership,
  preflight rejection, unknown-command, and no-replay regressions pass. The
  local gate now passes 47 shared-core and 58 host tests plus rustfmt, Clippy
  with warnings denied, the release build, Bash syntax, and diff checks.
- Read-only live inspection found `win11_gpu/ua-virtiomem0` converged at
  `requested=current=2105344 KiB`, but the installed controller had restarted
  30 times and was repeatedly reporting false `domain_xml` drift. Candidate
  service validation was subsequently completed; the active-controller
  reboot/interruption cases remain before TASK-022 can close.
- The single candidate-install/service-recovery batch timed out at the host's
  interactive sudo prompt before its script began, so it made no mutation and
  was not retried piecemeal. The read-only post-check found the old installed
  binary SHA-256 `3db1924d3cc131754be889ae3a2541e8cd866b1e4281a96ebc00017dbfd95039`,
  the service active after restart 31, and the allocation still converged at
  `2105344 KiB`.
- The operator then invoked the exact batch through sudo. Its XML extractor
  greedily removed the requested/current values and failed the converged-state
  assertion at line 66 before the intended mutation. Because rollback was
  armed too early, that error still caused an unnecessary clean service
  stop/start; it retained the old binary, reset `NRestarts=0`, and left the
  allocation unchanged. The extractor and rollback ordering are corrected.
  The batch now also stages the executable in `/usr/local/libexec`, restores
  its SELinux context, atomically renames it, verifies the installed hash
  before start, and reports the failed line/status on error.
- The corrected batch installed candidate SHA-256
  `a1c431e67b49ba0373091fb32760cecea38a7e311bdc04a8972a9a44bf2a607c`
  with `system_u:object_r:usr_t:s0`, restarted the unit, and observed it with
  `NRestarts=0`. It emitted exactly one `shrink_request_rejected` event for the
  known stale attestation, issued no shrink request or unknown-command event,
  retained `requested=current=2105344 KiB`, and removed both staged and backup
  files. This completes the candidate service/rejection/no-replay gate.
- The approved active-controller lifecycle batch requested one planned QGA
  reboot at 13:43:55. Windows became reachable, but `msiexec.exe` initiated an
  unrelated second planned reboot at 13:50:31 to finish configuring Sunshine.
  QEMU PID 379778 remained continuous from September 7, so this was not a KVM
  process crash. No current Kernel-Power 41, unexpected-shutdown 6008, or
  BugCheck 1001 record exists. During the two boot windows `dommemstat` omitted
  `unused` and the controller accumulated seven systemd restarts; it then
  recovered active, QGA and authenticated Windows SSH passed, and allocation
  remained converged at 2155872256 bytes. Old-attestation drift rejection after
  the final boot confirms refreshed attestation was not active, so this run
  does not close the reboot/interruption or attestation gates.
- Hardened the canonical privileged-batch prompt and project testing rules:
  guest lifecycle now requires hypervisor continuity, an independent Windows
  command and named service/application checks, pending reboot/installer
  inspection, correlated crash/reboot events, exactly one expected boot, and
  repeated end-to-end health over a quiet window before any persistent step.

## 2026-09-09 target-controller milestone redesign

- Added M10e to calculate an absolute desired virtio-mem allocation from fresh
  Windows availability/commit demand, explicit fixed/base memory, configurable
  reserves, rolling history, hysteresis, bounds, safe floors, and alignment.
- Added M10f to separate durable `desired`, device `requested`, and actual
  `current` state. Fresh pressure may safely raise or cancel a pending shrink;
  another lower request remains prohibited while reclaim is in flight.
- Added M10g to qualify the new controller hermetically and live, including
  growth by 4 GiB followed by a stable 2 GiB-extra target, zero/partial shrink,
  pressure interruption, stale input, command ambiguity, cancellation, and
  restart.
- Reclassified M10b's 30/60/120-second re-notification and 300-second deadline
  as a bounded diagnostic/recovery profile. Timeouts remain valid for I/O,
  freshness, and health reporting, but no timeout decides how much RAM Windows
  needs.
- Updated M11/M11a to consume absolute desired targets and constrained actual
  allocation rather than reproducing the current per-poll decrement loop.
- Made automatic shrink a default-on product capability in code and the host
  configuration example. Same-target re-notification remains default-off;
  freshness, warmed history, safe floors, bounded 64 MiB actuation, one-request
  convergence, and ambiguity/stall latching remain mandatory.
- Added the normative `docs/target-controller.md` contract with checked
  formulas, fixed-visible-base validation, effective device maximum, reserve
  defaults, 10-minute high-water history, 256 MiB downward hysteresis, M10f
  transitions/journaling, and separate M10g controller/platform qualification.

### M10e-M10g acceptance decisions

- **TASK-026 / M10e:** implement the normative estimator, named configuration,
  history checkpoint, immediate-growth/delayed-reclaim behavior, conservative
  restart warm-up, explicit capacity-limited health, and boundary/overflow/
  alignment tests. Exit requires deterministic +4 GiB then +2 GiB target
  traces without actuation-sized demand guesses.
- **TASK-027 / M10f:** implement explicit control health, bounded movement,
  upward-only pending-shrink supersession, stale-input freeze, partial-progress
  accounting, write-before-command intent, durable latch, restart resolution,
  and dry-run-default latch clearing. Exit requires no lower overlap and no
  command replay across every modeled result.
- **TASK-028 / M10g:** report controller qualification independently from
  platform reclaim qualification. Exit requires the complete hermetic matrix
  and bounded live committed-only/resident workload evidence with captured
  initial state and recovery. Automatic shrink stays default-on; zero/partial
  driver progress must become explicit constrained/latched health rather than
  silently changing the product to growth-only operation.
- Current local validation passes 55 shared-core and 60 host tests, formatting,
  warnings-as-errors Clippy, the core/host release build, Bash syntax, and diff
  checks. The Windows crate was not changed; its latest native gate remains 67
  tests.

## 2026-09-08 asymmetric service-quanta policy

- A controller-isolated live ramp grew `win11_gpu` from 1 GiB to 5 GiB in
  about four seconds, held it for 30 seconds, then lowered `requested` by
  256 MiB every five seconds. Windows reclaimed 3,000 MiB before stabilizing
  at 2,120 MiB with the final 1 GiB request still pending.
- The interrupted ramp was recovered to the observed stable current allocation
  (`requested=current=2222981120`) and the old controller remains stopped so
  it cannot resume its obsolete 2 MiB-step policy.
- Selected the replacement service rule: any growth decision emits one 1 GiB
  quantum; a release decision emits one 64 MiB quantum. Pressure severity does
  not multiply a request, both quanta must be live-block-aligned, and the next
  quantum remains convergence-gated.
- Added an explicit `guest-stats` compatibility mode for the validated
  pre-M10d live instance. Raw Windows telemetry remains the production default;
  the compatibility mode does not synthesize an envelope and reuses the same
  directional policy and guarded resize sink.
- Corrected the freshness adapter to accept libvirt's live `last_update`
  spelling while canonicalizing it with the documented `last-update` alias so
  either spelling works and a mixed duplicate still fails closed.
- Installed the candidate and observed its first exact 64 MiB automatic shrink
  request. `current` made zero progress for 300 seconds; the state machine
  latched with zero restarts. This guest therefore fails the selected "if it
  can shrink" condition, so automatic reclaim is disabled while 1 GiB growth
  remains enabled.
- Validation passes 46 shared-core, 50 host, and 67 native Windows tests plus
  release, format, and warnings-as-errors gates. The native artifact SHA-256 is
  `d91e6ccd2a0fdbac1da8bcd96a4e05ecf77964834e20d973c844e44d77d1e9dd`.

## 2026-09-08 M10 delivery and shrink-recovery implementation

- Completed TASK-021 code for atomic current-record publication, bounded
  three-record retention, durable restart-safe host replay acknowledgement,
  and LocalService ProgramData ACL provisioning. Native installed-ACL evidence
  remains because the current guest has no ProgramData directory and the
  candidate was not installed.
- Advanced TASK-022 with separate default-off shrink/re-notification controls,
  a fake-clock-tested 30/60/120-second three-notification state machine, an
  immutable 300-second deadline, progress and ambiguity handling, non-fatal
  latched stall, and cancellation/restart replay suppression.
- Added bounded Rust `qualify-shrink` and operator-only `abandon-shrink` paths.
  Recovery requires two stable samples, an immediate race-check read, one
  request to observed `current`, and convergence within 30 seconds.
- Validation passes 45 shared-core and 48 host tests, format, warnings-as-errors
  Clippy, release build, and Bash syntax. The native Windows build passes 67
  tests and produced verified SHA-256
  `ec4a965f468312615ec1416336f1f5674cd7fce34bbbf62f9a7bc55b3d5c0995`.
- The prepared `win11_gpu/ua-virtiomem0` one-block live batch stopped at the
  outer sudo password prompt and was cancelled without mutation. Read-only
  checks confirmed `requested=current=1 GiB`, both Windows services running,
  and the host controller active. Live M10b and installed ACL evidence remain.

## 2026-09-08 beta build and live-validation agent policy

- Updated the canonical agent instructions and every reusable task prompt so
  repository builds, tests, candidate installation, relevant service lifecycle
  checks, live inspection, and bounded reversible resize tests may run by
  default during beta qualification without a second approval pause.
- Live mutations still require an execution notice, an unambiguous target,
  existing compatibility/attestation/headroom gates, bounded convergence, and
  rollback to captured initial state. Privileged operations remain one
  task-specific script under one outer `sudo`, with direct operator password
  entry only.
- Reboots, deletion of pre-existing resources, persistent platform/security
  configuration changes, disabled safety controls, non-reversible resize, and
  unrelated mutations remain explicit-approval operations. Cleanup of
  temporary resources created by the same test is permitted by default.
- Validation: `git diff --check` passed and a repository prompt scan found no
  remaining rule that requires separate approval for task-scoped beta build,
  test, service lifecycle, or bounded reversible resize work.

## 2026-09-08 M10aX driver status-interface feasibility

- M10a3's no-progress shrink, the larger partial-progress shrink, and two
  empty bounded driver captures establish the concrete unmet diagnostic need:
  host evidence cannot distinguish notification receipt, branch selection, no
  removable Windows range, or a device/driver failure.
- Completed a proposal-only feasibility review for a versioned, buffered,
  read-only, administrator/SYSTEM-only status IOCTL backed by an internally
  consistent cached snapshot. It exposes bounded aggregate state and outcome
  counters, never memory contents, block maps, physical ranges, or controls.
- The proposal defines ABI evolution, access control, malformed-request and
  concurrency testing, external driver build/signing gates, disposable-guest
  qualification, exact package rollback, and protected-guest approval gates.
- Closed M10b evidence did not show that the extra diagnostic distinctions
  change an operational decision. Implementation remains No-Go without
  upstream/fork ownership, security review, a signing route, and rehearsed
  disposable-guest rollback.
  No driver code, build, install, guest/host mutation, or Rust consumer was
  added.

## 2026-09-08 M10d envelope/replay slice started

- Claimed TASK-021 and introduced raw-telemetry schema version 2 with explicit
  VM/service/session identity, Unix and monotonic milliseconds, session-local
  sequence, native Windows telemetry provenance, and a required host live-
  libvirt-current allocation join.
- The host now validates expected service identity, same-session ordering, new-
  session sequence zero, and a bounded set of 16 retired sessions. It rejects
  partial final records, records over 64 KiB, and files over 1 MiB before JSON
  parsing or policy evaluation.
- The gate passes 32 shared-core, 42 host, and 66 native Windows tests, plus
  formatting, warnings-as-errors Clippy, release builds, and Bash syntax. The
  checksum-verified Windows artifact SHA-256 is
  `4c28f41b7d57984bac1fad82461fddba18831cdf19945257cd795cddc84ca314`.
- TASK-021 remains in progress: implement publisher rotation/retention, atomic
  reader acknowledgement/handoff, durable restart-safe replay state, and
  least-privilege ProgramData/transport ACL provisioning and tests.

## 2026-09-08 M10c host-side current-allocation join

- Moved the platform-neutral raw telemetry and demand-calculation contract into
  `virtio-mem-core`. Version 1 raw records carry only VM identity, Unix
  observation seconds, and validated canonical-byte Windows counters.
- Wired interactive and SCM Windows paths to append raw telemetry without an
  allocation input or resize interface. Configuration schema version 3 adds
  the libvirt VM name used for the join.
- Added a host file source with injected-clock freshness checks and wrong-VM,
  malformed, missing, future, and stale rejection. The runtime joins the record
  with a newly read alias-scoped live XML `current`, calculates through shared
  policy, rejects geometry conflicts, and leaves shrink advisory pending M10b.
- The local gate passes 31 shared-core and 40 host tests, formatting, Clippy
  with warnings denied, release build, Bash syntax, and diff checks. The native
  Windows gate passes 66 tests, formatting, Clippy, and release build; the
  fetched executable SHA-256 is
  `c81405f3121c1479b57e100002637a5fd12225a6880baa6c6b5c0020e7bc87cc`.

## 2026-09-08 M9e host telemetry correctness and freshness

- Corrected `dommemstat` mapping so balloon `actual` is retained only as
  provenance, `unused` is free memory, and required `available` supplies the
  available/total-like legacy bound. `unused` and `available` may exceed
  `actual`; only `unused > available` is inconsistent.
- Added required 60-second maximum-age and 5-second future-skew example bounds,
  injected wall-clock tests, and per-source strict `last-update` advancement.
  Missing, stale, future, regressing, non-advancing, malformed, duplicate, and
  overflowing samples fail closed.
- Added the read-only Rust `decision` command using the service configuration,
  selected stats source, live alias-scoped XML, and exact runtime policy
  evaluator. Removed the duplicate QGA-only Bash preview.
- The native RHEL gate passes 29 core and 35 host tests, release build,
  formatting, Clippy with warnings denied, and Bash syntax. No live host,
  service, VM, or memory state was changed.

## 2026-09-07 M9d compatibility-attestation drift guard

- Replaced the static workload-review boolean with a required version-1 JSON
  attestation path for both the installed controller and explicit resize CLI.
- The SHA-256 fingerprint binds VM/alias identity, allocation-neutral live
  domain XML and native QEMU argv, alias-scoped QMP properties, QEMU/libvirt
  versions, slot/VFIO budgets, workload/device exclusions, balloon state,
  trusted-development classification, and Windows driver version.
- Added a read-only `attest VM ALIAS REVIEW_FILE` command that emits a reviewed
  candidate without installing it or actuating memory. Every resize rereads
  the protected file, verifies integrity, recollects fresh live evidence, and
  fails closed on missing, malformed, tampered, unsupported, or drifted input.
- Deterministic tests cover exact match, review/fingerprint tampering, identity,
  XML/QEMU/QMP/version drift, and allocation changes that must not revoke an
  otherwise unchanged configuration. The RHEL gate passes 29 core and 35 host
  tests, formatting, release build, Clippy with warnings denied, and Bash
  syntax. No live host, service, VM, or memory state was changed.

## 2026-09-07 M10b bounded retry/recovery policy selected

- Selected a default-off host state machine for Windows shrink qualification:
  five-second observation, exact-target re-notification after 30/60/120 seconds
  without block progress, at most three re-notifications, and one immutable
  300-second operation deadline.
- Progress updates its timestamp but never replenishes the retry budget or
  extends the deadline. External target changes, stale/invalid state,
  cancellation, restart, guest transition, and ambiguous command outcomes
  latch actuation off instead of replaying work.
- A stalled shrink remains observable without failing the worker or triggering
  a systemd restart loop. The preferred non-disruptive recovery to qualify is
  one abandon-to-current request after two stable samples and an immediate
  pre-apply read; it keeps partial reclaim and has one 30-second window with no
  retry.
- TASK-022 now requires fake-clock coverage before separate approved live
  same-target and abandon-to-current probes. Failed qualification leaves
  automatic shrink disabled; graceful domain recreation remains the final
  operator-approved fallback.

## 2026-09-07 M10b larger-shrink probe

- An approved controller-isolated probe grew the live device from 1 GiB to
  3 GiB. Growth converged in about two seconds and remained converged during a
  30-second hold.
- A subsequent 1 GiB shrink request targeted 2 GiB. Windows immediately made
  partial progress from 3 GiB to `2682257408` bytes: 257 of the requested 512
  blocks (514 MiB) were unplugged, leaving 255 blocks (510 MiB) above target.
- `requested=2147483648` and `current=2682257408` then remained unchanged for
  the rest of the 300-second window. This rules out the earlier 2 MiB request
  being rejected merely for being too small and supports a single-pass
  removability/no-retry explanation.
- No overlapping request was issued. Graceful domain recreation from the
  persistent 1 GiB definition restored `requested=current=1073741824`, fresh
  balloon statistics, running `viomem`, and the active controller with
  `NRestarts=0`.

## 2026-09-07 M10a3 bounded one-block observation

- Recovered the post-RHEL-reboot state before testing: stopped the controller's
  15-second failure loop, persisted virtio-balloon statistics at a five-second
  period, and persisted/restored the known 1 GiB virtio-mem baseline.
- The approved isolated operation grew `win11_gpu/ua-virtiomem0` by one 2 MiB
  block. Libvirt converged from `requested=current=1073741824` to
  `requested=current=1075838976` bytes.
- The predeclared recovery request set `requested=1073741824`, but `current`
  remained `1075838976` for all 60 five-second samples. The bounded 300-second
  window expired without another request. This directly demonstrates a
  Windows no-progress shrink and confirms that shrink is not rollback.
- Windows `viomem` remained running and `dommemstat` remained fresh. The
  checksum/signature-verified kernel capture emitted no matching filtered
  `Memory config` record, so installed-driver `requested_size`/`plugged_size`
  remain unavailable. Host `requested`/`current` remain allocation authority.
- DbgView's process, temporary service, registry keys, and staging directory
  were removed. A separately approved graceful QGA shutdown/start recreated
  the device from persistent XML and restored
  `requested=current=1073741824`. Fresh `dommemstat`, running `viomem`, and an
  active controller with `NRestarts=0` were independently verified. Persistent
  XML retains the 1 GiB request and balloon statistics period 5.

## 2026-09-05 M10a1 qualification and M10a2 evidence harness

- Downloaded DbgView 5.02 from Microsoft's Sysinternals endpoint and recorded
  package SHA-256 `a8454253756af10667b82faf2323de536f0b7084d732acba63803df01ce4c316`.
  The staged `dbgviewcli.exe` SHA-256 was
  `2d3128bf2338fa25873173f265504515801d1e76d6215efa511e00dc825c3cf7`,
  and Windows SignTool verified its Microsoft signature and timestamp chain.
- An approved 60-second/1,000-line/1-MiB no-resize kernel capture loaded
  `Dbgv.sys` and stopped on its duration bound. It did not enable boot logging,
  create a debug-print filter, restart `viomem`, reboot, resize, or change host
  controller state. No `Memory config` line appeared; this is inconclusive for
  informational-message filtering, not evidence that the driver emitted none.
- DbgView extracted `Dbgv.sys` to its working directory rather than the staged
  directory. An approved no-reboot recovery reintroduced only its temporary
  SCM key, let DbgViewCLI use its normal unload path, and removed the locked
  image and executable. `Dbgv` and both DbgView processes are absent;
  `viomem` remains running; live libvirt and the active controller remain
  converged at 1 GiB. The later M10a3 recovery removed the remaining empty
  `HKCU\Software\Sysinternals` container key, completing cleanup.
- Completed TASK-015/M10a2 with a versioned shared-core JSON evidence contract.
  It requires repeated operation/VM/device identity, explicit byte units,
  source identity, strict sequence/monotonic ordering, nondecreasing wall time,
  stable device geometry, converged before/after libvirt endpoints, Windows
  health, and controller state. Driver trace is optional diagnostic evidence.
- Seven new deterministic tests accept trace-present/trace-absent records and
  reject mixed identity, unit ambiguity, clock/sequence regression, missing
  layers/endpoints, divergent endpoints, geometry drift, and invalid trace
  values. The focused shared-core gate passes 29 tests and Clippy warnings-as-
  errors.

## 2026-09-05 M10a allocation-contract redesign

- Reframed M10a after reviewing Virtio 1.2, the pinned QEMU/libvirt contract,
  the matching virtio-win worker, and current Microsoft kernel-capture
  behavior. Device `requested_size`/`plugged_size` are read-only device
  configuration consumed by the driver; host `requested`/`current` represent
  requested intent and guest-cooperative allocation.
- Completed TASK-013 and TASK-017: alias-scoped live libvirt `current` is the
  host allocation authority. Driver debug output echoes device configuration
  and is diagnostic evidence, not a second accounting source or a prerequisite
  for M10c/M10d or hermetic M11 simulation.
- Kept TASK-015 Ready with a revised evidence contract: QEMU/libvirt state,
  Windows health, controller state, identity, units, ordering, and convergence
  are required; a driver trace record is optional.
- Reclassified TASK-014 and TASK-016 as optional operator-approved diagnostics.
  Initial DbgView qualification must account for Windows debug-print filtering
  and must not enable boot logging, persist debug-filter registry changes,
  restart the driver, or reboot the guest.
- Removed the assumption that a 2 MiB growth followed by shrink is guaranteed
  reversible. Any live observation needs a predeclared non-convergence recovery
  target and should be rehearsed on a disposable guest when practical.
- M10b still blocks automatic shrink and live multi-target actuation. M11
  hermetic simulation may proceed once M9e and M10d provide trustworthy inputs;
  live expansion also requires M9d and M10b.

## 2026-09-05 upstream virtio-mem audit and milestone update

- Completed TASK-024, a pinned review of the virtio-mem site, QEMU/libvirt
  documentation and QGA schemas, and the Windows `viomem` source corresponding
  to the installed driver.
- Reopened ISSUE-006 and added M9e/TASK-025: `dommemstat actual` is balloon
  state rather than whole-guest allocation, `available > actual` is valid, and
  the current adapter does not check `last-update` freshness.
- Confirmed `guest-get-memory-stats` is not an upstream QGA command. The
  existing adapter remains an experimental custom/downstream boundary; an
  upstream QGA upgrade is not a remedy.
- Expanded M9d/TASK-019 to fingerprint backend, memory-slot/VFIO budget,
  balloon-resize, incompatible-workload, topology, trust, and version evidence.
- Selected the M10c/TASK-020 architecture: Windows publishes a fresh raw
  telemetry envelope; the host joins alias-scoped live libvirt `current` and
  calculates the target. No host allocation feed or guest actuation is added.
- Expanded M10b/TASK-022 with Windows shrink retry/recovery qualification and
  a required default-off automatic-shrink control until that evidence passes.
- Restricted Phase 2 to one active controller/device on the development host;
  M11 owns multi-target reservation. `win11_gpu` is a fully trusted
  development/test guest, so a hard QEMU/libvirt memory limit is recommended
  defense-in-depth here and mandatory for future untrusted/production guests.
- Full findings and source pins are in
  `docs/upstream-virtio-mem-audit.md`. No live host or guest state changed.

## 2026-09-05 whole-roadmap reconciliation

- Reconciled every project document against the current Rust implementation
  and live M7–M10a evidence. Current platform gates are recorded separately as
  22 shared-core, 29 host, and 64 Windows tests.
- Closed stale board claims about missing configuration selection, unit
  conversion, bounded shutdown, SCM observation, QGA-backed Windows production
  polling, and guest resize wiring.
- Added M9d/TASK-019 for compatibility-attestation drift, M10c/TASK-020 for
  current-allocation ownership, M10d/TASK-021 for report freshness and bounded
  delivery, and M10b/TASK-022 for the single-VM failure/recovery matrix.
- M10a2/TASK-015 is now Ready and hermetic; its evidence parser does not depend
  on privileged M10a1 capture. The later M10a redesign made driver trace an
  optional evidence layer and removed it as an accounting dependency.
- Dated handoffs remain historical evidence and are subordinate to the current
  snapshot in `PROJECT_STATUS.md`.

## 2026-09-05 M10a cross-layer state observation started

- Claimed M10a after M9b completed with `win11_gpu` converged at a retained
  1 GiB allocation. This task must observe the same controlled operation at
  the Windows `viomem.sys` driver and QEMU/libvirt layers before treating
  driver `requested_size`/`plugged_size` as equivalent to host
  `requested`/`current`.
- Initial scope is read-only discovery of an existing supported driver-state
  surface and a bounded evidence procedure. Direct driver IOCTL additions,
  driver forks/builds/signing/installation, kernel debugging changes, and any
  further live resize remain out of scope without their own design and
  explicit approval.
- Read-only discovery on `ice101.lan` found the signed, running Red Hat driver
  `100.102.104.29400` and its `PCI\\VEN_1AF4&DEV_1058` device. PnP properties,
  service/driver metadata, registry device parameters, Event Log channels, and
  registered trace providers expose no `requested_size` or `plugged_size`.
- The contemporaneous upstream `mm314` source has no IOCTL, WMI, or performance-
  counter query path. It does contain a configuration-change debug message
  with both fields, but `EVENT_TRACING` is disabled and the shipped binary
  routes the message to kernel debug output. The string is present in the live
  binary; no compatible debugger/capture tool is installed on the guest.
- At this point in the investigation M10a was treated as blocked at the
  protected-guest boundary. The later allocation-contract redesign supersedes
  that conclusion: bounded capture is optional diagnostic evidence, and
  aggregate Windows physical memory remains pressure telemetry rather than an
  allocation substitute.
- The same discovery pass recorded a non-mutating baseline: live libvirt
  remained converged at `requested=current=1073741824` bytes and Windows
  reported 9,148 MB of aggregate physical memory.
- Microsoft's signed Sysinternals `dbgviewcli.exe` is the preferred bounded
  capture candidate, but kernel capture requires Administrator rights and
  automatically loads `Dbgv.sys`. Tool staging, capture, controller stop,
  one-block resize/rollback, artifact retrieval, cleanup, and controller
  restoration therefore require one explicit operational approval.
- The initial umbrella split M10a into capture qualification, correlation,
  one-block observation, and contract adoption. The later redesign completed
  the protocol/source contract and retained capture/observation as optional
  diagnostics; M10aX now requires a concrete unmet diagnostic need.
- The umbrella and children use new TASK-013 through TASK-018 identifiers;
  this removes the accidental reuse of TASK-010, which remains the stable ID
  of the completed Rust host CLI migration.

## 2026-09-05 M9b live-controller start

- The native RHEL gate passes with 22 core tests and 29 host tests after the
  M9b change; initial live `win11_gpu` state was converged at
  `requested=current=0`, with a 2 MiB block and working `dommemstat` fields.
- Discovery found an already enabled `virtio-mem-host@win11_gpu.service` in a
  persistent 15-second restart loop. Its installed 2026-08-18 binary predates
  the zero-state fix and rejects the valid fully unplugged state; the current
  repository binary has not yet been installed.
- The shared controller policy also rejected a converged allocation below its
  configured minimum, which made the documented zero-to-1-GiB installed-service
  bootstrap impossible. M9b now treats that case as one aligned request to the
  configured minimum while retaining convergence, compatibility, workload,
  device-headroom, and host-headroom gates. Normal policy remains one block at
  a time, and above-maximum state still fails closed.
- One approved privileged batch stopped the stale loop, retained protected
  backups, installed the checksum-verified current binary and a fixed
  `win11_gpu` configuration, and started the existing enabled instance.
- The installed controller requested the aligned 1 GiB minimum and converged
  at `requested=current=1073741824`. A further poll interval retained the
  target, the unit is enabled and active with `NRestarts=0`, and rollback did
  not run. M9b is complete; interruption/reboot and cross-layer driver evidence
  remain M10a/M10b work.

## 2026-09-05 M9a live compatibility gate

- Added a bounded Rust QMP compatibility source that reads
  `dynamic-memslots` and `unplugged-inaccessible` from the selected live QOM
  device before every prepared resize. Disabled, malformed, missing, and
  conflicting evidence remains fail-closed.
- Added required systemd configuration `VIRTIO_MEM_WORKLOAD_REVIEWED`; only
  literal `true` records the operator review as confirmed. The CLI retains its
  explicit `--workload-reviewed` acknowledgement.
- Live QMP on `win11_gpu` reported `dynamic-memslots=true` and
  `unplugged-inaccessible=on`. The 2 MiB block matches the 2 MiB host THP PMD
  size and native QEMU arguments report `mem-lock=off`.
- The reviewed VFIO devices are an NVIDIA GPU, its audio function, and an AMD
  USB controller; none is NVMe. No RDMA or vhost-user indicator was found, and
  the operator confirmed those workload classes are not intended.
- The Rust CLI produced the exact one-block dry-run vector without `--apply`;
  identical before/after XML SHA-256 values prove non-mutation. M9a is
  complete; applied resize/convergence remains M9b/M10b scope.

## 2026-09-05 M9 live Rust XML-adapter validation

- Fixed the shared state contract to accept a fully unplugged live
  virtio-mem device (`requested=current=0`) while continuing to reject a zero
  resize target. Added direct state and captured-XML regression tests.
- The native RHEL gate passes with 21 core tests and 25 host tests, release
  build, formatting, warnings-as-errors Clippy, and Bash syntax.
- One approved read-only Rust CLI batch selected `ua-virtiomem0` from live
  `win11_gpu` XML and reported size 20 GiB, block 2 MiB, and converged
  `requested=current=0` in canonical bytes.
- The live CLI rejected a nonexistent alias and rejected a one-block dry run
  because `dynamic-memslots` and `unplugged-inaccessible` evidence is unknown.
  Identical before/after XML SHA-256 values prove the batch did not mutate the
  domain. M9 is complete; M9a compatibility evidence remains.

## 2026-09-04 M8 fresh read-only QGA/KVM validation

- Ran one approved, password-once privileged read batch against only
  `qemu:///system` and `win11_gpu`; it made no host, VM, service, XML, or
  memory changes and required no rollback.
- QGA `110.0.2` answered three `guest-info` requests in 77–124 ms, plus
  `guest-ping`, `guest-get-osinfo`, and `guest-get-host-name`; the guest is
  Windows 11 x64 with hostname `ICE101`.
- `guest-get-memory-stats` remains explicitly unsupported. Three
  `dommemstat` fallback samples completed in 84–129 ms with numeric `actual`,
  `unused`, and `available` fields.
- Live XML confirmed the connected `org.qemu.guest_agent.0` channel and
  `ua-virtiomem0` at a 20 GiB maximum, 2 MiB block size, and converged
  `requested=current=0`.
- The Windows service no longer opens the QGA pipe, so LocalService pipe ACLs
  are not an M8 dependency. A controlled `qemu-ga` stop/start passed, a
  graceful QGA-mode reboot completed at Windows boot time 23:54:19, and the
  post-reboot host probe repeated all QGA, fallback, channel, and convergence
  checks successfully. M8 is complete.

## 2026-09-04 single-prompt privileged RHEL task workflow

- Added a reusable agent prompt for consolidating an approved task's RHEL
  `virsh`, `systemctl`, `journalctl`, and related inspection commands into one
  reviewable temporary Bash script and one outer `sudo` invocation.
- Task scripts live under the ignored `.vscode-artifacts/privileged-tasks/`
  directory, contain no nested sudo or password handling, and are not standing
  authorization for later tasks.
- The operator still approves the exact batch and types the sudo password
  directly; no credential cache, passwordless sudoers rule, or stored password
  is introduced.

## 2026-08-18 M9c Rust-only host control migration

- Added M9c to replace `scripts/virtio-mem-host.sh` with an authoritative Rust
  CLI for snapshot, validation, dry-run, and explicitly applied resize
  operations.
- The Rust CLI must reuse the existing XML, compatibility, unit, convergence,
  and host-headroom contracts rather than reimplementing policy.
- The Bash helper is explicitly a temporary migration target and must be
  removed after equivalent Rust regression and operator validation.
- This milestone is intentionally before broader live resize automation so the
  repository has one host-control implementation and one safety policy.

## 2026-08-18 M9 host adapter boundary alignment

- Aligned `scripts/virtio-mem-host.sh` with the Rust resize sink's safety
  contract: resize mode now requires explicit live XML confirmation of
  `dynamic-memslots` and `unplugged-inaccessible`, plus an operator-confirmed
  incompatible-workload review.
- Snapshot mode remains read-only and can inspect the current VM without those
  actuation acknowledgements.
- The helper defaults to `qemu:///system` and supports `VIRSH_CONNECT` for an
  explicitly selected alternative connection.
- The current `win11_gpu` XML is expected to fail closed because the required
  compatibility attributes are absent; no live resize was attempted.

## 2026-08-18 M9a compatibility gate started

- Added `VirtioMemCompatibility` with explicit `Confirmed`, `Rejected`, and
  `Unknown` evidence states for XML flags and workload review.
- Captured XML parses `dynamic-memslots`/`dynamicMemslots` and
  `unplugged-inaccessible`/`unpluggedInaccessible` attributes when present.
- `VirshResizeSink` now fails closed before `update-memory-device` unless both
  required flags and workload review are confirmed. Independent evidence can
  complete unknown XML fields, while conflicting evidence is rejected.
- Added regression coverage for confirmed attributes, unknown evidence,
  independent evidence merge, and conflicts.
- The live `win11_gpu` XML lacks these attributes, so the gate correctly blocks
  live resizing. `domxml-to-native qemu-argv` was unavailable, and workload or
  incompatible-device absence cannot be inferred from the memory XML alone.
- Local evidence: formatting, 17 core tests, 16 host tests, and focused Clippy
  pass.

## 2026-08-18 V2 live virtio-mem XML inspection

- Read-only inspection of `win11_gpu` over `qemu:///system` found exactly one
  selected virtio-mem device: alias `ua-virtiomem0`.
- Live XML reports `size=20971520 KiB` (20 GiB), `block=2048 KiB` (2 MiB),
  `requested=1048576 KiB`, and `current=1048576 KiB`; three convergence
  rechecks remained equal at 1 GiB.
- The device uses shared `memfd` backing. The captured XML does not expose
  `dynamic-memslots` or `unplugged-inaccessible`, and native QEMU argument
  conversion was unavailable, so the compatibility state remains unknown.
- No prohibited device/workload class was positively identified from the
  inspected XML, but XML alone cannot prove absence of `mlock`, RDMA
  migration, or vhost-user workload dependencies.
- Updated `scripts/virtio-mem-host.sh` to default to `qemu:///system` with a
  `VIRSH_CONNECT` override. No resize or other mutation was attempted.

## 2026-08-18 M8/V1 read-only guest validation

- Completed the explicit `win11_gpu` RHEL read-only probe using
  `qemu:///system`.
- Host prerequisites passed; the domain is `running`; libvirt is 11.10.0,
  QEMU API is 11.10.0, hypervisor is 10.1.0, and QGA is 110.0.2.
- `guest-info` succeeded and the `org.qemu.guest_agent.0` virtio-serial
  channel is connected at the expected libvirt path.
- Three consecutive `dommemstat` fallback samples passed with
  `actual=8388608 KiB`, `unused=4676156 KiB`, and
  `available=9367852 KiB`.
- Approximate full `virsh` command latencies were 3913 ms for `guest-info`
  and 3374 ms for `dommemstat`; these include local libvirt/virsh startup
  overhead and are not isolated QGA wire latency.
- The connected QGA explicitly reports that `guest-get-memory-stats` is not
  implemented. The helper now passes V1 using the documented dommemstat
  fallback and defaults to `qemu:///system` with `VIRSH_CONNECT` override.
- No guest command, reboot, resize, service installation, or systemd/libvirt
  mutation was attempted. Windows-side QGA service state, pipe ACL behavior,
  and native guest-agent memory-stat support remain open.

## 2026-08-18 F8 concrete runtime wiring

- Completed the interactive `run` path with validated configuration,
  native Windows telemetry worker construction, and bounded `ServiceHost`
  execution.
- Preserved the same native telemetry boundary in SCM startup; the SCM path
  does not open the QGA virtio-serial device or issue resize requests.
- Added typed `RuntimeWiringError` context for configuration validation,
  worker construction, and service-host execution, plus explicit SCM stage
  labels for worker construction, initialization, and host execution.
- Confirmed deterministic fake state-provider and resize-sink coverage remains
  available for the legacy shared polling harness; production demand
  publication awaits TASK-020/TASK-021 and no guest resize wiring is planned.

## 2026-08-18 F5 guest transport boundary hardening

- Added a stable correlation id to the Windows
  `guest-get-memory-stats` request.
- Added strict response validation for the Windows transport: exactly one
  newline-delimited frame, matching response id, required `return` array, and
  explicit rejection of empty, multiple, malformed, and QGA error envelopes.
- Preserved the host adapter's compatibility parser for responses without an
  id because host QGA is a separate transport boundary.
- Added deterministic captured-envelope and framing regression tests.
- The live Windows pipe path, ACL behavior, QGA service state, and three
  consecutive guest probes remain blocked by the external KVM validation gate.

## 2026-08-18 F6a contract and unit safety

- Added shared checked canonical-byte/KiB conversion helpers and exact
  round-trip/maximum-boundary tests.
- Strengthened `VirtioMemState` to require a block-aligned device size in
  addition to aligned requested/current/target values, with the existing
  zero, range, minimum-block, power-of-two, and headroom checks preserved.
- Applied the 1 MiB/power-of-two block contract to shared policy configuration
  and reused checked conversion in the host resize sink.
- Changed `/proc/meminfo` conversion from saturating to checked multiplication;
  overflow now fails closed.
- Local evidence: `cargo fmt --all -- --check`, 15 core tests, 16 host tests,
  and focused Clippy pass. The complete Linux workspace remains blocked by
  the pre-existing Windows-only SCM compilation boundary; validate the full
  workspace on Windows/MSVC.
- Compatibility flags (`dynamic-memslots`, `unplugged-inaccessible`) and
  incompatible workload/device evidence remain open under M9a because they
  require live QEMU/libvirt configuration and operator evidence.

## 2026-08-18 `guest-get-memory-stats` live capability check

- Confirmed on the running `win11_gpu` guest that QGA `110.0.2` does not
  advertise `guest-get-memory-stats` in `guest-info`.
- A direct request still fails with `The command guest-get-memory-stats has
  not been found`.
- The repository implementation is already complete in the shared parser,
  Windows QGA boundary, host `VirshGuestAgent`, and validation helper. No
  additional Rust implementation can add a command that the external Windows
  QGA binary does not provide.
- Enabling the command requires installing a Windows QGA build that includes
  the memory-stats command, restarting the guest QGA service, and repeating
  the three-sample validation. Keep the active host controller on the
  verified `dommemstat` source until that guest-side change is complete.

## 2026-08-18 RHEL controller installation and live validation

- With explicit approval, created the dedicated `virtio-mem-host` system
  account and `libvirt` supplementary-group access, installed the release
  binary and templated systemd unit, deployed the `win11_gpu` configuration,
  and enabled `virtio-mem-host@win11_gpu.service`.
- Performed the approved reversible live grow of `ua-virtiomem0` from `0 KiB`
  to `1073741824` bytes (1 GiB); forward convergence completed after two
  samples and the target remains retained for controller validation.
- Fixed the systemd unit to select `qemu:///system` and provide a writable
  runtime cache directory for libvirt. The service account is `uid=972`,
  primary group `virtio-mem-host`, supplementary group `libvirt`.
- Live validation found that this Windows guest reports `dommemstat
  available` above `actual` while `unused` remains valid. Updated the parser
  to use conservative `unused` fallback semantics for that counter shape and
  added a regression test.
- Host validation passed after the fix: formatting, 15 host tests, release
  build, and Clippy warnings-as-errors. The service is enabled and active;
  latest journal entries contain no runtime errors.
- Final live state: `ua-virtiomem0` has `requested=current=1048576 KiB`.

## 2026-08-18 RHEL read-only recheck

- Read-only validation against `win11_gpu` completed successfully on the RHEL
  host: the domain is running, libvirt is 11.10.0, and QEMU reports API
  11.10.0 / hypervisor 10.1.0.
- `virsh dommemstat win11_gpu` reports `actual=8388608 KiB`,
  `available=8319276 KiB`, and `usable=3398070 KiB`; `dommemstat` remains a
  usable controller source.
- QEMU Guest Agent responds at version `110.0.2`, but
  `guest-get-memory-stats` remains unsupported. Keep
  `VIRTIO_MEM_STATS_SOURCE=dommemstat`.
- Live XML reports alias `ua-virtiomem0`, 20 GiB maximum, 2 MiB block, and
  `requested=current=0 KiB`; the device is converged and no resize was
  attempted.
- `virtio-mem-host@win11_gpu.service` is not installed, enabled, or active;
  no journal entries exist. Installation, account setup, configuration
  deployment, and service start remain explicitly approved host mutations.
- Corrected `host/systemd/virtio-mem-host.conf.example` to use the live alias
  `ua-virtiomem0`.

## 2026-08-17 Documentation Handoff

Imported applicable Windows service guidance from Microsoft Learn into
`docs/architecture.md`, `docs/engineering-standards.md`, `docs/testing.md`,
and `windows/README.md`. The documented lifecycle contract is intended for
the remaining SCM adapter work; no unfinished SCM capability is marked as
complete by this documentation update.

Execution source of truth. Update after every session.

## 2026-08-18 Windows service runtime boundary fix

- Confirmed the running `QEMU-GA` service owns the Windows virtio-serial
  endpoint; a second guest process receives `open ...: 5`.
- Added `NativeTelemetryWorker` and wired interactive/SCM startup to native
  `GlobalMemoryStatusEx`/`GetPerformanceInfo` collection, so Windows service
  startup no longer depends on opening the QGA device.
- Retained the QGA client and parser as explicit adapter/test boundaries;
  RHEL/libvirt remains the owner of host-side QGA requests.
- Added native-worker startup/failure regression tests. Workspace tests now
  pass with 71 tests and no failures.

## 2026-08-18 running-service RHEL handoff

- The guest is running and the QGA channel responds to `guest-info` and
  `guest-get-host-name` (`ICE101`). The RHEL host cannot directly observe
  Windows SCM state; service-running confirmation remains Windows-side
  evidence.
- `guest-get-memory-stats` remains unavailable, but this is not a Windows
  service startup blocker because the current service uses native
  `GlobalMemoryStatusEx`/`GetPerformanceInfo` telemetry and does not open the
  QGA virtio-serial device.
- `virsh dommemstat win11_gpu` remains available. A fresh post-driver-update
  XML check reports virtio-mem `requested=0 KiB` and `current=0 KiB`, so the
  previous convergence blocker is resolved. No resize or other VM mutation
  was attempted.

## 2026-08-18 driver-update convergence handoff

- After the latest Windows driver was installed, the RHEL read-only check
  observed `requested=0 KiB` and `current=0 KiB` for `ua-virtiomem0`.
- ISSUE-005 is resolved as an observed convergence result. Direct driver
  `requested_size`/`plugged_size` telemetry is still not exposed through the
  checked QGA commands. Later protocol/source review established host
  allocation authority without using QGA or driver debug output as a duplicate
  state feed.
- The host controller remains uninstalled, and no resize was attempted in
  this verification.

## 2026-08-18 Windows QGA device-path fix

- Foreground service diagnostics reproduced startup failure as
  `wait for \\.\Global\org.qemu.guest_agent.0: 161`.
- Fixed `windows/src/qga.rs` to skip `WaitNamedPipeW` for the QEMU
  virtio-serial device path under `\\.\Global\`; `WaitNamedPipeW` only accepts
  `\\.\pipe\...` endpoints and returned `ERROR_BAD_PATHNAME`.
- Added a regression test distinguishing Win32 named-pipe paths from the
  QEMU device path. Formatting, Clippy, and all 69 workspace tests pass.
- Rebuild and redeploy the Windows service binary before repeating SCM start.

## 2026-08-18 guest-get-memory-stats clarification — superseded by 2026-09-05 upstream audit

- Confirmed that the custom `guest-get-memory-stats` adapter is implemented end
  to end in the repository: the Windows named-pipe client sends the newline-
  delimited request, the shared parser validates `stat-free`, `stat-total`, and
  optional `stat-available`, and the host `virsh` adapter sends the same exact
  command.
- Workspace validation passed with 78 tests and no failures.
- No Rust change is required to add the custom adapter. The later pinned audit
  established that upstream QGA does not define the command, so an upstream
  upgrade is not a remedy. Enable it only with an exact validated downstream
  implementation; otherwise use `dommemstat` subject to M9e.

## 2026-08-18 Windows QGA/build handoff

- Confirmed `windows/src/qga.rs` already sends the newline-delimited
  `guest-get-memory-stats` request with a bounded overlapped named-pipe
  operation; no repository-side QGA request implementation is missing.
- Built the RHEL host controller successfully as
  `target/release/virtio-mem-host`; `cargo test -p virtio-mem-host` passed all
  14 tests.
- A Windows service artifact could not be produced on this RHEL host because
  `x86_64-pc-windows-gnu` is not installed and no MinGW, Clang, LLD, or MSVC
  linker is available. Build `virtio-mem-service` on the Windows guest or a
  Windows build host, then validate the QGA command there.
- The live QGA still does not advertise `guest-get-memory-stats`. The later
  upstream audit confirms that this is expected; only a custom/downstream
  implementation could provide it. The host `dommemstat` adapter remains the
  default and is implemented, with correctness/freshness work in TASK-025.

## 2026-08-18 privileged-probe batching handoff

- Strengthened repository agent guidance so related privileged read-only host
  probes are approved and executed as one batch under one outer `sudo`.
- The agent must not invoke `sudo` once per `virsh`/systemd probe, depend on
  sudo timestamp caching, or retry authorization separately for each command.
- Passwords remain operator-entered directly into the terminal and are never
  requested, stored, or transmitted to the agent.

## 2026-08-18 RHEL read-only validation handoff

- A single unprivileged read-only batch completed without repeated password
  prompts: `bash scripts/check-environment.sh`, `virsh version`, domain state,
  `dommemstat`, XML, QGA capability checks, systemd status, and journal read.
- `bash scripts/check-environment.sh` passed on the RHEL host.
- `bash scripts/validate-guest-agent.sh win11_gpu 3` reached `guest-info`,
  then failed closed because QGA 109.1.0 does not implement
  `guest-get-memory-stats`.
- `virsh dommemstat win11_gpu` succeeded with `actual=8388608 KiB`,
  `unused=4137384 KiB`, and `available=8337708 KiB`; the default
  `dommemstat` source therefore has the required fields for this guest.
- `virtio-mem-host@win11_gpu.service` is not installed and has no journal
  entries. Installing it remains an explicitly approved mutation.
- A fresh read-only recheck confirms the QGA responses do not expose Windows
  driver `requested_size`/`plugged_size` fields; driver evidence must come
  through a separately validated observation path.
- `virsh dumpxml win11_gpu` reports the alias, size, block, requested, and
  current values used by the host convergence gate. No resize, service,
  systemd, or VM mutation was attempted.
- Corrected the host XML discovery contract to use `virsh dumpxml <vm>`;
  this libvirt version rejects the unsupported live-option form.
- Hardened `dommemstat` parsing to reject KiB-to-byte overflow instead of
  saturating, with regression tests.

## 2026-08-18 Documentation synchronization

- Synchronized `PROJECT_STATUS.md` and `IMPLEMENTATION_PLAN.md` with the
  current Phase 2 roadmap and backlog; removed obsolete task numbering and
  early-scaffold claims.
- Corrected the feature matrix and API contract to describe implemented native
  telemetry, advisory demand reports, and validated JSON configuration while
  retaining live-validation and production-wiring gaps.
- Corrected roadmap milestone wording so operation deadlines and configured
  shutdown-timeout enforcement remain explicitly open.
- Replaced the root `readme.md` with a user-facing guide covering project
  goals, architecture, current status, safety boundaries, quick start, and
  links to the roadmap and supporting contracts.
- Local documentation and Rust validation remain required before marking any
  implementation milestone complete.
- Added a version-2 QGA operation timeout and native overlapped named-pipe
  boundary for connect/write/read. A timed-out request now calls `CancelIoEx`,
  closes its handles, and returns an explicit transport error without using
  the non-cancellable synchronous flush API.
- Added a bounded `ServiceHost` worker boundary using the configured shutdown
  timeout, typed timeout failure, SCM timeout wiring, and deterministic slow-
  worker tests. Live install/start/stop observation remains separate.

## 2026-08-18 Windows demand-agent handoff

- Added `windows/src/demand.rs` with native `GlobalMemoryStatusEx` and
  `GetPerformanceInfo` collection, checked canonical-byte counters, a versioned
  advisory demand report, bounded provisional pressure states, aligned target
  limits, and a conservative safe floor.
- Added deterministic tests for invalid counters, pressure bounds, state
  classification, target clamping/alignment, and invalid current allocation.
- The collector/report are not wired to host actuation. QGA/dommemstat and the
  existing host controller remain unchanged and authoritative for resize.
- Added a one-cycle `DemandAgent` collection/publication boundary with injected
  publisher, explicit telemetry/publication errors, and deterministic tests.
- Added validated durable JSON-lines output and a generic stoppable advisory
  worker; the configured path defaults under `C:\ProgramData`.
- Added versioned JSON service configuration loading with validated defaults
  when the file is absent, explicit schema rejection, and startup integration.
- Live Windows workload evidence, persistent/event report output, and main
  SCM construction remain open until a trustworthy current-allocation provider
  and its service-account ACLs are validated.

## Documentation Freshness Rules

After completing any task:

1. Update the task card status below
2. Update [docs/feature-matrix.md](docs/feature-matrix.md) if features changed
3. Update [docs/roadmap.md](docs/roadmap.md) if phase status changed
4. Update [docs/issues.md](docs/issues.md) if bugs were resolved
5. Document any handoff notes or blockers in the task card
6. Move completed tasks to the **Completed** section

### 2026-09-04 VS Code RHEL-to-Windows build workflow

- Added `scripts/windows-remote-build.sh` for explicit SSH connectivity,
  source synchronization, native Windows MSVC build/test/lint, artifact fetch,
  and cross-host SHA-256 verification.
- Added checked-in `.vscode/tasks.json` tasks for the local RHEL gate and
  non-mutating Windows build workflow, including an SSH-alias prompt and a
  single-sync native aggregate gate. Only task and launch JSON are unignored;
  endpoint-specific settings remain outside the repository.
- Locked remote Cargo operations to `Cargo.lock`, required a successful release
  build before artifact retrieval, and included Bash syntax validation in the
  RHEL gate.
- Corrected the RHEL gate to validate only the platform-neutral core and host
  controller; the Windows SCM crate is validated by the native Windows gate.
- Limited source transfer to Git-tracked and non-ignored files so working-tree
  edits are included without copying ignored local credentials or build output.
- Updated the README, Windows README, dependency matrix, testing guide,
  feature matrix, and roadmap with the workflow and safety boundary.
- Initial local evidence: shell syntax, task JSON parsing, Rust formatting, the
  core and host release build, 38 core/host tests, and warnings-as-errors
  Clippy passed before the Windows endpoint run documented below.
- No libvirt, systemd, SCM, guest lifecycle, or live memory mutation was
  attempted; remote writes were limited to the designated build workspace.

### 2026-09-04 Windows endpoint bootstrap and milestone runner

- Enabled the Windows OpenSSH Server for automatic startup on the private KVM
  guest interface and installed the operator-supplied build key using the
  hardened administrator key-file ACL.
- Confirmed the ED25519 host fingerprint, active TCP/22 listener, Rust 1.97.1
  MSVC target, Visual Studio 2022 x64 build environment, and native Windows
  release build, formatting, 59 tests, and warnings-as-errors Clippy gate.
- Added `scripts/complete-windows-build-milestone.sh` to pin an explicitly
  supplied host fingerprint in a temporary known-hosts file, check the remote
  toolchain, run `make all-gates` twice, and retain logs plus artifact hashes in
  the ignored artifact directory.
- Extended the remote wrapper with optional pinned known-hosts and identity-file
  inputs. Neither the host key nor private key is copied into the repository or
  persistent SSH configuration.
- Local validation passes: `bash -n scripts/*.sh`, ShellCheck for both affected
  scripts, the Linux-compatible release build, 38 core/host tests, Clippy with
  warnings denied, and the native Windows MSVC release build, 59 tests,
  formatting, and Clippy with warnings denied.
- TASK-011 milestone helper completed two consecutive aggregate runs. Evidence
  is retained under ignored directory
  `.vscode-artifacts/windows/milestone-20260904T212207Z/`; both artifact hashes
  are `56572af65f9a9297a5ff755dd01e7a9908d93c037a25cf8419b3113d9ae4b16e`.
- The first direct aggregate run from RHEL now passes: native MSVC release
  build, 59 Windows tests and doctests, rustfmt, warnings-as-errors Clippy, and
  SHA-256-verified artifact
  `56572af65f9a9297a5ff755dd01e7a9908d93c037a25cf8419b3113d9ae4b16e`.
- The wrapper resolves real Rust toolchain binaries because this endpoint's
  rustup proxy symlinks fail through OpenSSH with Windows error 448; it also
  normalizes `certutil.exe` carriage returns before checksum parsing.

### 2026-09-04 M9c Rust-only host CLI

- Added authoritative Rust `snapshot`, `validate`, and `resize` subcommands
  for one explicit VM and alias. Resize is a dry run unless `--apply` is
  supplied and reports its complete `virsh` argument vector.
- Reused the shared XML, compatibility, convergence, canonical-unit, device-
  headroom, and host-headroom contracts. The operator must explicitly confirm
  the incompatible-workload review and provide a positive host reserve.
- Added hermetic regression tests proving snapshot/validation are read-only,
  dry-run sends no update, apply sends exactly one prepared update, and
  insufficient host headroom fails before actuation.
- Removed `scripts/virtio-mem-host.sh` without leaving a replacement copy of
  its XML parsing, arithmetic, compatibility, or resize policy in Bash.
- Local evidence: release build, rustfmt check, 19 core tests, 25 host tests,
  warnings-as-errors Clippy, and Bash syntax all pass. No live libvirt or VM
  mutation was attempted.

### 2026-09-04 M7 recovery observability foundation

- Added a native Windows Application Event Log sink with stable lifecycle and
  failure IDs, bounded single-line messages, and an explicit publication
  boundary.
- SCM startup, running, stop request, clean stop, configuration failure,
  worker failure, unexpected stopless exit, and status-publication failure now
  produce distinct records.
- A worker that exits without cancellation is now a non-zero SCM failure;
  successful completion after stop/shutdown remains zero-exit. Deterministic
  tests cover both classifications and failure after cancellation.
- The native Windows release build, 64 tests, rustfmt, and warnings-as-errors
  Clippy pass. The fetched executable SHA-256 is
  `2dc1bf9df309e86c39119890cad8c7419de831e3276ba36b920535a72ed26c8e`.
- Live `ice101.lan` validation passed candidate install/start/observe/stop/
  remove under `LocalService`. Events 1000–1003 and exit zero proved the clean
  lifecycle; invalid configuration emitted event 2000, exited one, and started
  a second process 5.05 seconds later under the configured recovery policy.
- Live validation found and fixed pre-dispatch configuration loading, which
  previously produced SCM error 1053 without service status or event context.
  `windows/src/main.rs` now attaches to SCM before configuration loading and
  has a startup-route regression test.
- Classic `/f:text` rendering is not authoritative without a registered
  message resource; XML EventData contains the correct bounded strings. This
  packaging gap is tracked as ISSUE-008.
- Rollback restored the original binary SHA-256
  `5ac0f46e402606a9a71f95318b6338f1649879b1b5389a54e992b3dae9e459d3`,
  service security descriptor, recovery configuration, absent ProgramData
  directory, and running state. The temporary backup was removed.

## Ready Queue

Tasks ready to start (Phase 2 - Core Functionality):

| ID | Title | Owner | Status | Effort | Dependencies |
| --- | --- | --- | --- | --- | --- |

## In Progress

| ID | Title | Owner | Status | Handoff Notes |
| --- | --- | --- | --- | --- |
| TASK-028 | M10g single-VM target-controller qualification | Copilot + Operator | In Progress | Hermetic and candidate no-actuation gates pass. Production raw transport/install, refreshed attestation, and bounded committed/resident workload actuation remain. |
| TASK-009 | Windows native demand-agent foundation | Copilot | In Progress | Native telemetry, advisory calculation, raw production publication, M10c join, and M10d bounded delivery are implemented. Installed ProgramData ACL verification and live workload tuning remain. |

## Planned

| ID | Milestone | Owner | Status | Depends on | Exit evidence |
| --- | --- | --- | --- | --- | --- |
| M11 | Global pool simulation using absolute targets | Copilot | Planned | M10f | Deterministic multi-VM accounting/arbitration tests pass |
| M11a | Target-based controlled reclaim | Copilot + Operator | Planned | M10g, M11 | Simulation and bounded live gates prove supersession, constrained progress, and host reserve safety |

### 2026-08-18 live KVM handoff

- First read-only checks against `win11_gpu` on the RHEL server passed for
  `virsh`/`jq`; libvirt reports 11.10.0, the guest is running, and the QGA
  channel `org.qemu.guest_agent.0` is connected.
- `guest-info`, `guest-ping`, `guest-get-osinfo`, and
  `guest-get-host-name` returned valid responses. The guest reports QGA
  `109.1.0`, Windows 11 x64, and hostname `ICE101`.
- The repository probe is blocked because this agent does not advertise or
  implement `guest-get-memory-stats`; it returns `has not been found`. No
  guest command execution, reboot, resize, or XML mutation was attempted.
- Read-only XML inspection found virtio-mem alias `ua-virtiomem0`, size 20 GiB,
  block 2 MiB, and `requested=current=0`. Automatic resize remains disabled.
- `qemu-system-x86_64` is not in the current PATH; verify the host package/path
  separately before relying on direct QEMU CLI diagnostics.

### 2026-08-18 Windows SCM validation handoff

- The release service binary built successfully.
- The approved `VirtioMemService` install attempt could not open the Windows
  SCM because the current terminal lacks service-manager permissions.
- A read-only `sc.exe query VirtioMemService` confirmed no partial service
  registration exists. Do not mark F7/M7 complete until an administrator runs
  the documented install → start → observe → stop → remove sequence.
- Follow-up administrator run used `sc.exe` correctly: registration succeeded,
  but `sc.exe start VirtioMemService` returned `ERROR_ACCESS_DENIED (5)` and
  the service remained stopped. `sc.exe stop` returned the expected `1062`
  because it was never started; removal then succeeded. The likely deployment
  blocker is the default `LocalService` account lacking traversal/read access
  to the executable under `C:\Users\Dave\github`; validate this before changing
  the least-privilege account or service security descriptor.
- Elevated VS Code rerun confirmed the same result with explicit `sc.exe`:
  release build, install, query, stop, and removal completed; start failed with
  error 5. Read-only `icacls` confirmed the executable grants access to SYSTEM,
  Administrators, and the developer account, but not `NT AUTHORITY\LocalService`.
  No ACL was changed automatically. The service registration was removed after
  the test.

### 2026-08-18 Windows SCM deployment follow-up

- The release binary was copied to `C:\Program Files\VirtioMemService` and
  `LocalService` was granted recursive read/execute access.
- The service reached `RUNNING` with a reported PID, stopped cleanly, and was
  removed successfully. The final `sc.exe query` returned expected error
  `1060`.
- Event-log visibility, recovery actions, and QGA access under `LocalService`
  remain open.

### 2026-08-18 M6/F8 QGA runtime wiring — superseded by native telemetry

- Interactive and SCM workers now construct `NamedPipeGuestAgent` from the
  validated configuration, apply the configured QGA operation timeout, and
  acquire/parse memory statistics during initialization and polling.
- QGA transport/parser failures now fail the worker visibly; no resize sink is
  connected and no virtio-mem `current` allocation is inferred from QGA stats.
- Deterministic worker tests cover successful initial acquisition and explicit
  transport failure. This was later replaced in production by native telemetry;
  host actuation remains separate and no guest resize sink is planned.

### 2026-08-18 Windows service hardening

- SCM installation now applies the configured service description and bounded
  restart actions at 5, 30, and 60 seconds with a 24-hour reset period.
- Failure actions are enabled for non-crash failures so unexpected worker
  exits can be recovered without treating an intentional stop as a crash.
- Live recovery behavior, Event Log visibility, and QGA access under
  `LocalService` remain validation work; no automatic recovery fault was
  induced during this session.
- Corrected the VS Code SCM task artifact path: workspace release builds are
  emitted under `target\release`, not `windows\target\release`. The corrected
  binary showed the configured description and 5/30/60-second failure actions;
  startup reached `START_PENDING` and then stopped with exit code 1 because the
  guest QGA memory-stat command is unavailable. Cleanup removed the service.

### 2026-08-18 shell-safety and privilege handoff

- Added repository-wide prompt rules requiring explicit current-turn approval
  before editing/deleting protected server files or mutating VM, libvirt, or
  systemd state.
- Prompt rules forbid `sudo`, `su`, `doas`, password collection, and password
  automation. Normal Cargo/Bash validation remains unprivileged.
- Documented the recommended one-time administrator setup for a dedicated
  least-privilege `virtio-mem-host` account instead of repeated root prompts or
  broad passwordless sudo access.
- Added `scripts/preview-memory-decision.sh`, a read-only policy preview (later
  removed when M9e moved the preview onto the shared Rust controller path) that
  reports no-change, blocked, grow, or shrink decisions and never issues a
  live resize command.
- Added `scripts/live-resize-test.sh`, which requires explicit `--apply`, logs
  requested/current convergence over time, and restores the original size by
  default after a successful test.
- Added explicit `--connect` support after the first server shell was found to
  default to the empty `qemu:///session` connection while the VM lives under
  `qemu:///system`.
- Updated prompt and testing rules: privileged scripts require current-turn
  approval naming the complete command, target, mutation, and rollback, then
  run once as a whole under `sudo`; passwords remain operator-entered only.
- The approved 20 GiB attempt was rejected by `virsh` before mutation because
  the adapter passed canonical bytes to a KiB-valued `--requested-size` option.
  The VM remained at `requested=current=0`; the script and Rust resize sink now
  convert exact byte values to KiB and reject lossy conversions.
- Incident follow-up: the live test now rejects full-device targets, defaults
  to an 8 GiB target cap, and requires 4 GiB host `MemAvailable` headroom after
  the increase. Automatic controller operation remains disabled pending an
  equivalent host-capacity safety gate.
- The live test records forward and restoration convergence independently and
  reports restoration only after `requested == current` is confirmed.
- Follow-up read-only check: `win11_gpu` is running on `qemu:///system`, its
  QGA channel is connected, and virtio-mem remains `requested=current=0`. The
  host controller systemd unit is not installed/running, and QGA still lacks
  `guest-get-memory-stats`; the Windows service cannot be confirmed through
  the non-command QGA probes. The hardened 20 GiB dry run returned blocked
  before any mutation.
- The live test now separates forward convergence timeout from rollback
  timeout. A 30-second test timeout cannot shorten the default 300-second
  rollback window.
- AI-run live tests have a hard 30-second forward timeout and compact terminal
  output; detailed polling remains in the optional CSV log. Restoration retains
  a separate 300-second default.
- Added a fixed 1 GiB retention floor to the live test: smaller targets are
  rejected and restoration never requests below 1 GiB.

### 2026-08-18 host controller stats-source and safety-gate hardening

- Fixed ISSUE-001 in code: `HostConfig` now selects a memory-stat source with
  `VIRTIO_MEM_STATS_SOURCE` (`dommemstat` by default, `qga` opt-in). The new
  `host/src/dommemstat.rs` reads virtio-balloon-backed `virsh dommemstat`
  counters (`actual`/`unused`/`available`) so the controller no longer depends
  on the unimplemented `guest-get-memory-stats` QGA command. Whether the
  connected guest's balloon driver actually reports `unused`/`available` on
  `win11_gpu` still needs a live, read-only `virsh dommemstat win11_gpu` check
  before automatic operation is enabled.
- Added a hard `MIN_HEADROOM_BYTES` (1 GiB) invariant to the shared
  `VirtioMemState::validate_target` in `virtio-mem-core`, so no resize target
  (Windows or host) can ever be validated within 1 GiB of the device's full
  size, independent of operator-configured `max_memory_bytes`.
- Added a host-side memory-headroom gate: `host/src/host_memory.rs` reads
  `/proc/meminfo`'s `MemAvailable`, and `HostRuntime` now skips (does not
  error, just logs and waits) any grow decision unless the RHEL host has
  enough free memory for the requested delta plus the new
  `VIRTIO_MEM_HOST_MIN_HEADROOM_BYTES` reserve, mirroring
  `scripts/live-resize-test.sh --host-reserve-bytes`.
- Workspace `cargo fmt --all -- --check`, `cargo test --workspace`, and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` all
  pass locally after these changes (52 tests).
- Not done in this session (requires live RHEL/libvirt access and explicit
  operator approval, which this session did not have): building/installing
  the `virtio-mem-host` systemd unit, creating the least-privilege
  `virtio-mem-host` account, and running a live test through the installed
  service.

## Completed

| ID | Title | Owner | Completed | Notes |
| --- | --- | --- | --- | --- |
| TASK-027 | M10f desired/requested/current reconciler | Copilot | 2026-09-09 | Explicit control health, bounded movement, upward-only pending-shrink supersession, stale-input freeze, partial-progress accounting, atomic write-before-command intent, durable restart/no-replay resolution and latching, and dry-run-default `clear-latch` pass 62 core and 65 host tests. |
| TASK-026 | M10e quantitative desired-allocation model | Copilot | 2026-09-09 | Absolute checked physical/commit targets, base/effective-maximum gates, normal/floor reserves, bounded warmed history, hysteresis, capacity-limited output, atomic fingerprint-bound checkpointing, conservative restart, and +4 GiB then +2 GiB traces pass 55 core and 60 host tests. |
| TASK-022 | M10b single-VM failure, Windows shrink, and recovery matrix | Copilot + Operator | 2026-09-09 | Operator accepted closure from 47 core/58 host hermetic tests, installed candidate rejection/no-replay, bounded one-block retry and abandon-to-current recovery, and partial/no-progress live shrink evidence. The unexecuted clean reboot/refreshed-attestation batch remains optional and is not claimed as passed. |
| TASK-021 | M10d demand envelope and bounded delivery | Copilot | 2026-09-08 | Version 2 identity/provenance, atomic current-record handoff, three-record retention, durable restart-safe replay acknowledgement, read bounds, and LocalService ProgramData ACL provisioning pass 45 core/48 host/67 native Windows tests; installed ACL verification remains operational evidence. |
| TASK-020 | M10c host-side current-allocation join | Copilot | 2026-09-08 | Windows publishes VM/time-scoped raw telemetry without allocation input; the host rejects invalid/stale/wrong-VM records, joins fresh alias-scoped live `current`, and calculates through shared policy. 31 core, 40 host, and 66 native Windows tests pass. |
| TASK-025 | M9e host telemetry correctness and freshness | Copilot | 2026-09-08 | Correct balloon mapping, bounded advancing `last-update`, injected-clock failures, and the shared-path Rust `decision` preview pass hermetic tests; the QGA-only Bash preview is removed. |
| TASK-019 | M9d complete compatibility-attestation drift guard | Copilot | 2026-09-07 | Version-1 SHA-256 evidence binds the full reviewed live configuration and operator declarations; every resize recollects evidence and fails closed on tamper or drift, with allocation-neutral hermetic tests. |
| TASK-001 | Rust service scaffolding | Copilot | 2026-09-04 | Service lifecycle, configuration, SCM adapter, native telemetry worker, legacy QGA adapter boundary, cancellation, error handling, and live SCM validation are complete; demand publication continues under TASK-009. |
| TASK-002 | QEMU Guest Agent validation | Copilot + Operator | 2026-09-04 | Repeated live advertised-QGA and `dommemstat` probes passed across agent restart and guest reboot; later audit classified the QGA memory adapter as custom and moved `dommemstat` semantics/freshness to TASK-025. |
| TASK-003 | Bash validation helpers | Copilot | 2026-08-17 | Added prerequisite, QGA probe, and Rust validation scripts. |
| TASK-004 | Windows memory polling policy | Copilot | 2026-08-18 | Parser, policy, adapter loop, wakeable polling, and service-hosting tests pass; production telemetry publication is tracked by TASK-009/TASK-020. |
| TASK-005 | Safe QEMU Guest Agent response handling | Copilot | 2026-08-18 | Framing, response correlation, malformed input, bounded overlapped I/O, operation timeout, and cancellation are tested as an adapter boundary; Windows production telemetry does not open QGA. |
| TASK-006 | Rust Copilot prompt set | Copilot | 2026-08-17 | Added repository-aware Rust project, API, test, refactor, security, docs, CI, and performance prompts; updated existing prompts and always-on instructions. |
| TASK-007 | Documentation review of libvirt/QEMU virtio-mem constraints | Copilot | 2026-08-18 | Added host-side virtio-mem semantics, compatibility limits, and live validation guidance based on official libvirt and QEMU documentation. |
| TASK-008 | RHEL virtio-mem host controller | Copilot + Operator | 2026-09-05 | M9/M9a/M9b passed on `win11_gpu`: installed current Rust controller, fresh XML/QMP and workload gates, zero-to-1-GiB bootstrap, convergence, retained minimum, active systemd instance, and 51 passing core/host tests. |
| TASK-011 | RHEL-controlled cross-platform developer gate | Operator + Copilot | 2026-09-04 | Two fingerprint-pinned aggregate runs passed: 38 RHEL core/host tests, 59 native Windows tests, release builds, formatting, warnings-as-errors Clippy, and matching verified artifact hashes. |
| TASK-010 | Rust host CLI replaces Bash resize helper | Copilot | 2026-09-04 | Rust owns alias-scoped snapshot/validation, exact dry-run arguments, explicitly applied one-shot resize, and shared safety gates; 44 core/host tests pass and the duplicate Bash helper is removed. |
| TASK-012 | Windows installation and recovery operations | Copilot | 2026-09-04 | LocalService install/start/observe/stop/delete and rollback passed; events 1000–1003, failure event 2000, exit codes, and the first 5-second recovery restart were verified live. |
| TASK-023 | Whole-roadmap documentation reconciliation | Copilot | 2026-09-05 | Reconciled ownership, current evidence, test counts, milestones, blockers, task states, and stale historical claims across all project documentation. |
| TASK-024 | Upstream virtio-mem deep audit | Copilot | 2026-09-05 | Pinned and reconciled upstream QEMU/libvirt/QGA/virtio-win constraints; added M9e/TASK-025, expanded M9d/M10b/M11, selected the M10c host-side join, and documented the trusted development-only support boundary. |
| TASK-013 | M10a allocation-authority contract | Copilot | 2026-09-05 | Virtio 1.2 and pinned implementation sources establish requested/plugged semantics; alias-scoped live libvirt `current` is authoritative and driver tracing is diagnostic. |
| TASK-017 | M10a4 state-contract adoption | Copilot | 2026-09-05 | Architecture, API, data, testing, roadmap, status, issue, and feature docs adopt host allocation authority and decouple optional driver tracing from accounting/simulation. |
| TASK-015 | M10a2 correlated behavior-evidence harness | Copilot | 2026-09-05 | Added bounded version-1 shared-core JSON validation for repeated identity, explicit bytes, ordered timestamps, required host/Windows/controller layers, stable geometry, converged endpoints, and optional aligned driver diagnostics; seven focused tests pass. |
| TASK-014 | M10a1 optional driver diagnostic qualification | Copilot + Operator | 2026-09-07 | Signed bounded no-resize capture and exact driver/process/service/file/registry cleanup passed without persistent debug configuration; no matching informational record appeared. |
| TASK-016 | M10a3 optional bounded driver observation | Copilot + Operator | 2026-09-07 | One-block grow converged; recovery shrink stayed divergent for 60 samples/300 seconds without overlap; a graceful domain recreation then restored 1 GiB convergence, healthy telemetry, and the controller with zero restarts. |
| TASK-018 | M10aX driver status-interface feasibility | Copilot | 2026-09-08 | Proposal defines a versioned cached read-only status IOCTL, administrator/SYSTEM ACL, compatibility, hostile-input/concurrency tests, external signing/install, and exact rollback gates. Closed M10b evidence did not justify implementation; it remains No-Go without external driver ownership and the other proposal gates. |

## Blocked

No current implementation task is blocked solely by the absence of a
user-mode `viomem.sys` state query. The completed TASK-018 feasibility proposal
does not authorize driver work or a protected-guest trial.

## Architecture Decisions

### 2026-09-05 upstream audit decisions

- `win11_gpu` is a fully trusted development/test-only KVM guest. Windows
  virtio-mem is treated as technology preview, not production support.
- A hard QEMU/libvirt cgroup memory limit is recommended defense-in-depth for
  this trusted guest and mandatory for any future untrusted/production guest.
- M10c uses a host-side join: Windows publishes fresh raw telemetry, while the
  host joins alias-scoped live libvirt `current` and calculates the target.
- `guest-get-memory-stats` is a custom/downstream adapter contract, not an
  upstream QGA capability. Upstream QGA remains a health/identity channel.
- `dommemstat actual` is balloon state; M9e now corrects its mapping and
  enforces source freshness before policy evaluation.
- Phase 2 permits one active controller/device on the development host. M11
  must provide atomic global reservation before multi-target actuation.
- Windows automatic shrink defaults enabled as a core product capability;
  explicit `false` remains available for diagnosis or rollout pause. M10b
  proved bounded diagnosis/recovery behavior, while M10e-M10g add quantitative
  sizing, warmed-history eligibility, durable reconciliation, and qualification.

### 2026-08-18 demand-agent and global-controller design review

The architecture review of the Windows driver findings is now incorporated as
the implementation direction for the next phases:

- Phase 2 builds a Windows demand-agent foundation using native
  `GlobalMemoryStatusEx` and `GetPerformanceInfo` telemetry.
- The Windows service reports measurements and recommendations; it does not
  directly allocate host memory or issue Linux/libvirt commands.
- The existing one-VM host controller remains the Phase 2 actuation authority
  and retains host-headroom, alignment, minimum-headroom, and convergence gates.
- Phase 3 adds one Linux global pool authority with host reserve accounting,
  multi-VM growth/reclaim priorities, trend-aware reclaim, and explicit pressure
  states.
- Upstream `viomem.sys` confirms block bitmaps, `requested_size` versus
  `plugged_size`, NUMA feature negotiation, inaccessible unplugged memory, and
  Windows memory-manager hot-add/hot-remove. These are integration boundaries,
  not a reason to duplicate driver mechanics in the Rust service.
- The upstream device interface does not establish a supported user-mode
  IOCTL/status contract for this repository; direct driver access is deferred.
- The upstream `viomem` project is a KMDF/Visual Studio solution with
  VirtIO/WDF library dependencies and Win10/Win11 x86/x64/ARM64 configurations.
  This repository should consume the driver as an external dependency, not
  absorb its kernel build, signing, or installation lifecycle.
- Upstream issue `#1574` is not cited as a viomem unplug defect; it is a
  `vioscsi` TRIM issue. Any viomem wait/non-convergence risk requires separate
  evidence.

New tracked work is documented in `docs/roadmap.md`,
`docs/future-architecture.md`, `docs/api-contract.md`,
`docs/data-model.md`, and `docs/testing.md`. QGA capability remains a
host-source compatibility detail; the Windows service uses native telemetry.

### Phase 2 handoff

- Implement native telemetry behind deterministic fakes first.
- Keep the demand report versioned and canonical-byte based.
- Do not replace QGA/dommemstat or enable new live resize behavior until live
  compatibility evidence exists.

### Phase 3 handoff

- Preserve alias-scoped live libvirt `current` as allocation authority and
  re-audit the pinned protocol/source mapping after stack upgrades.
- Build and test global pool arbitration in simulation before live multi-VM
  actuation.
- Require bounded, aligned, convergent reclaim with measured workload history.
- If a driver status interface is needed, create a separate signed-driver
  work item with access-control and rollback evidence before adding Rust calls.

### Runtime language policy

- Rust is the default choice for any service or program logic.
- Bash is used for automation and validation scripts.
- Go is explicitly not used in this repository.

### Communication Protocol

Use QEMU Guest Agent over the validated guest-host interface. Alternatives rejected:

Use QEMU Guest Agent over the validated guest-host interface. Alternatives rejected:

- Direct registry access: violates service boundaries
- Unvalidated custom protocols: adds unnecessary complexity
- Go-based implementation: intentionally excluded

### RHEL host controller

- A Rust controller supervised by a templated systemd unit manages exactly one
  explicitly configured VM and virtio-mem alias per service instance.
- It queries QEMU Guest Agent memory statistics and live libvirt XML, then may
  issue one validated `virsh update-memory-device --live` request.
- It must not discover VMs broadly, invoke a shell, administer Windows
  processes, or issue another request until `requested` equals `current`.
- Failed QGA, XML, or resize operations remain explicit service failures;
  systemd restart behavior must be bounded and must never blindly replay a
  resize request.
