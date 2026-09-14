# Implementation Plan

[`docs/roadmap.md`](docs/roadmap.md) is the normative architecture and
milestone path. `BACKLOG.md` owns the current task claim. This document maps the
roadmap to implementation-sized slices without duplicating milestone contracts.

## Current checkpoint

WN3 / TASK-038 is complete as the accepted construction checkpoint. The host
integrates separate memory-requirement, pressure-state, and shrink-safety
assessments with accepted telemetry and fresh allocation while explicit shadow
mode always returns `NoChange`. Separate fingerprint/history/status state,
append-only comparisons, and the typed no-resize workflow are implemented.
Focused, aggregate local, and native Windows gates pass; the shadow host and
schema-v3 Windows candidates are installed, and a schema-v2 fallback resident
run passed with unchanged allocation. By operator decision on 2026-09-14, the
remaining schema-v3 workload correlation and classification breadth moves to
later pressure-policy qualification. It is not a test pass or release evidence.
WN4 / TASK-039 is the next ready Windows construction slice.

HPM1 / TASK-033 is complete as a source-level durable-accounting milestone.
Shared core now owns the version-1 bounded, checksum-protected atomic pool
ledger, canonical complete-policy fingerprint, revision-checked plan ownership,
pre-dispatch growth reservation, observed reclaim accounting, and no-replay
restart classification. The focused core suite and aggregate local gate pass.
HPM2 / TASK-046 is the next host-pool slice after its WN3 dependency; no
runtime coordinator, resize dispatch, native, deployment, or live behavior
changed in HPM1.

The Windows sequence produces one guest-demand provider and preserves the
per-VM safety machinery. It is not the top-level product. HPM0-HPM6 add the
mandatory host-wide VM RAM pool, minimum guarantees, priorities, arbitration,
reclaim-for-transfer, additional guest-OS providers, and final release gate.

## Ordered implementation slices

1. **WN1 — Contract.** Add versioned types and validation for notification
   state, capability, availability, error, warm-up, timestamps, and optional
   snapshot/rate fields. Preserve allocation-free Windows telemetry.
2. **WN2 — Authoritative state.** Collect Windows low/high memory resource
   notifications through the cancellable service lifecycle and publish them
   through the existing atomic transport. Keep policy unchanged.
3. **WN3 — Shadow decisions.** Implement separate memory-requirement,
   pressure-state, and shrink-safety results. Join them with live
   `requested`/`current`, expose `demand_target` and reasons, and compare them
   with the temporary fixed-headroom baseline without actuation.
4. **WN4 — Growth.** Enable bounded normal and faster-growth modes using the
   pressure-aware assessment. Keep reclaim disabled and preserve every current
   host-capacity, attestation, journal, convergence, and latch gate.
5. **WN5 — Reclaim.** Add sustained shrink eligibility, blockers, hysteresis,
   safe floor, bounded shrink steps, and renewed-pressure cancellation. Fallback
   evidence never authorizes shrink.
6. **WN6 — Rates.** Add page-output evidence first. Add other Windows rates only
   if shadow results show independent value. Rates affect urgency and shrink
   blocking, not required-byte calculation.
7. **WN7 — Workloads.** Extend the Windows workload and qualification analyzer
   to distinguish resident, commit-only, cache/standby, modified, paging,
   burst, steady-state, and recovery behavior.
8. **WN8 — Endurance.** Run the reviewed detached repeated-cycle and unattended
   plans with resumable, immutable evidence and explicit final-state handling.
9. **WN9 — Provider configuration and cleanup.** Freeze validated Windows
   provider settings, fingerprints, migration, deployment templates, upgrade,
   downgrade, and rollback. Remove the threshold demand mode, fixed-headroom
   estimator/configuration, and expired decoder after their named gates close.
10. **WN10 — Provider decision.** Freeze an immutable Windows demand-provider
    and per-VM reconciler candidate and execute its code, native Windows,
    deployment, live, recovery, endurance, security, and operations gates. This
    is component readiness, not the final host-manager release.
11. **HPM0 — Pool contract.** Define total VM-pool bytes, explicit membership,
    total-RAM per-VM minimum/maximum/priority/provider, OS-neutral
    `GuestDemandReport`, non-reclaimable base accounting, and pool-grant/device
    target conversion. Define priority as contention-only: it creates no
    above-minimum reservation, and reclaim requires unmet higher-priority demand
    plus a strictly lower-priority safe donor. Audit the existing pure planner;
    do not change runtime behavior or freeze file syntax.
12. **HPM1 — Durable ledger.** Persist one atomic member-bound ledger in which
    current allocation, growth reservation, and observed reclaim account for
    every byte exactly once. Prove restart and ambiguity behavior without live
    actuation.
13. **HPM2 — Shadow coordinator.** Run one exclusive host process over a
    complete member snapshot, adapt Windows assessment into the common report,
    and emit deterministic grants/constraints without reaching a resize sink.
14. **HPM3 — Pool growth.** Make the pool grant the only source of per-VM
    `desired`, grant all growth that fits regardless of priority, use priority
    only for constrained contenders, reserve before dispatch, qualify one-VM
    and concurrent growth, then delete direct per-VM capacity allocation.
15. **HPM4 — Reclaim and transfer.** For an unmet higher-priority request,
    select only strictly lower-priority underutilised shrink-safe donors,
    reclaim no lower than `max(minimum, safe_floor)`, wait for observed release
    before recipient growth, then delete independent reclaim.
16. **HPM5 — Additional provider.** Add a Linux-native provider through the
    common demand report and qualify homogeneous and mixed-provider pools
    without OS-specific branches in arbitration.
17. **HPM6 — System release.** Freeze pool/member configuration and operations,
    complete multi-VM recovery/endurance, delete expired compatibility paths,
    and record the final host RAM manager GO or NO-GO decision.

## Cross-cutting implementation rules

- Guest services collect and normalise only. Host provider adapters assess
  demand; the host-pool manager grants targets and the host reconciler acts.
- Keep required-memory bytes, pressure state, and shrink safety separate in
  types, tests, status, and evidence.
- Keep total-RAM `demand_target` and `pool_grant` distinct from device-scoped
  `desired`.
- Reserve every enabled VM's minimum and all accepted growth before dispatch;
  do not count reclaim until authoritative live `current` confirms release.
- Let all VMs grow while capacity is available. Priority affects only
  contention and never creates an above-minimum reservation or proactive
  reclaim.
- Require a named unmet higher-priority recipient before reclaiming a strictly
  lower-priority donor. If none can release safely, keep the recipient waiting.
- Keep guest-OS-specific evidence behind provider adapters. Global arbitration
  uses demand, priority, minimum, safe floor, allocation, and freshness only.
- Prefer supported Windows APIs and counters. Do not synthesize a composite
  score when a native state exists.
- Keep every operational size, threshold, step, margin, window, and timeout in
  validated configuration or explicit run input.
- Add one signal family at a time and prove its semantics before granting it
  policy authority.
- Preserve telemetry identity/freshness/replay, compatibility, host reserve,
  alignment, desired/requested/current reconciliation, intent journaling,
  partial-progress accounting, and durable latches.
- Use temporary fallback or decoder paths only for a named qualification or
  rollback window. Delete them when their replacement gate passes; do not add
  permanent legacy/version-selection modes.
- Delete direct per-VM capacity allocation at HPM3 and independent per-VM
  reclaim at HPM4 after their respective qualification gates pass.

## Validation rule

For each slice, run focused tests in the owning crate and then
`cargo xtask gate local`. Run native Windows validation when collection or
Windows lifecycle changes. Deployment, live, recovery, and endurance results
remain separate and are required only when the milestone crosses those
boundaries. Historical or shadow evidence never counts as applied proof.
