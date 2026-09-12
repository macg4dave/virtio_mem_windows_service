# Implementation Plan

[`docs/roadmap.md`](docs/roadmap.md) is the normative architecture and
milestone path. `BACKLOG.md` owns the current task claim. This document maps the
roadmap to implementation-sized slices without duplicating milestone contracts.

## Current checkpoint

WN1 / TASK-036 is complete. The additive schema-v3 shared contract and its
explicit schema-v2 fallback passed focused, local, and native Windows gates
without new native collection, target-policy changes, deployment, or actuation.
The next slice is TASK-037 / WN2. The installed controller remains disabled and
inactive.

## Ordered implementation slices

1. **WN1 — Contract.** Add versioned types and validation for notification
   state, capability, availability, error, warm-up, timestamps, and optional
   snapshot/rate fields. Preserve allocation-free Windows telemetry.
2. **WN2 — Authoritative state.** Collect Windows low/high memory resource
   notifications through the cancellable service lifecycle and publish them
   through the existing atomic transport. Keep policy unchanged.
3. **WN3 — Shadow decisions.** Implement separate memory-requirement,
   pressure-state, and shrink-safety results. Join them with live
   `requested`/`current`, expose reasons, and compare them with the legacy
   candidate without actuation.
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
9. **WN9 — Configuration.** Freeze validated configuration, defaults,
   fingerprints, schema migration, fallback behavior, deployment templates,
   upgrade, downgrade, and rollback.
10. **WN10 — Release decision.** Freeze an immutable candidate and execute the
    complete code, native Windows, deployment, live, recovery, endurance,
    security, operations, and applicable host-capacity gates.

## Cross-cutting implementation rules

- Windows collects and normalises only. The host calculates targets and acts.
- Keep required-memory bytes, pressure state, and shrink safety separate in
  types, tests, status, and evidence.
- Prefer supported Windows APIs and counters. Do not synthesize a composite
  score when a native state exists.
- Keep every operational size, threshold, step, margin, window, and timeout in
  validated configuration or explicit run input.
- Add one signal family at a time and prove its semantics before granting it
  policy authority.
- Preserve telemetry identity/freshness/replay, compatibility, host reserve,
  alignment, desired/requested/current reconciliation, intent journaling,
  partial-progress accounting, and durable latches.
- Remove deprecated fixed-headroom and legacy demand paths only after the new
  schema, fallback, migration, and rollback behavior are proved.

## Validation rule

For each slice, run focused tests in the owning crate and then
`cargo xtask gate local`. Run native Windows validation when collection or
Windows lifecycle changes. Deployment, live, recovery, and endurance results
remain separate and are required only when the milestone crosses those
boundaries. Historical or shadow evidence never counts as applied proof.
