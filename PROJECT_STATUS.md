# Project Status

Updated: 2026-09-14

## Summary

The product destination is now explicit: one host-wide VM RAM manager owns a
configured total-guest-RAM pool, per-VM minimum guarantees and priorities,
durable reservations, contention arbitration, and safe reclaim-for-transfer.
Guest operating systems only report native demand/pressure through host-side
provider adapters. The Windows-native work is the first provider beneath that
manager, not the final allocation architecture.

The intended priority policy is contention-only. Lower-priority VMs may use
available pool RAM normally. Reclaim occurs only for unmet higher-priority
demand, only from strictly lower-priority shrink-safe donors, and never creates
recipient capacity before the release is observed.

Basic Windows telemetry, the host-side allocation join, compatibility
attestation, fixed-headroom targets, durable per-VM reconciliation, bounded
recovery, and the HPM0 host-pool contracts and pure planner are implemented. The
fixed-headroom formula is a temporary migration baseline, not a supported
long-term policy. The completed coherent deployment evidence remains valid.
The controller remains disabled/inactive, and automatic resizing remains
NO-GO until the pressure-aware provider and host-pool paths are implemented and
qualified.

## Current work

- TASK-035 completed the implementation/document audit, Microsoft-native signal
  research, target architecture, obsolete-item classification, and WN0 cleanup.
- TASK-038 / WN3 is complete as the accepted construction checkpoint. Explicit
  shadow mode joins accepted telemetry to live allocation, always suppresses
  actuation, persists versioned state/comparisons, and has a typed no-resize
  workflow. Focused, aggregate local, and native Windows gates pass; deployed
  schema-v2 fallback qualification preserved allocation and the current
  schema-v3 service is installed with advancing telemetry. The remaining
  workload-correlation breadth is deferred to later pressure qualification and
  is not release evidence.
- TASK-028 and QA-T009 are paused as fixed-headroom qualification. Their
  evidence remains useful for reconciliation and platform behavior but cannot
  qualify the replacement demand policy.
- WN4 / TASK-039 is in progress. Shared core selects bounded normal, urgent,
  fallback, held, and capacity-limited growth; explicit host `growth` mode
  persists that decision and independently suppresses every lower request
  before the resize sink. Focused core/host tests and the aggregate local gate
  pass. Native, deployment, and QA-T028 applied evidence remain outstanding.
- HPM0 / TASK-032 is complete at the source-contract layer. Versioned semantic
  pool/member and provider-neutral demand-report types, checked total-RAM
  accounting, lifecycle holds, contention-only priority, and named safe donor
  plans passed focused and aggregate local validation. HPM1 / TASK-033 is also
  complete at the durable-ledger source layer; HPM2 runtime coordination has
  not started.

The installed host controller remains disabled/inactive. No WN3 qualification
issued a resize, and automatic resizing remains NO-GO.

## Implemented

- Windows own-process SCM lifecycle with validated required configuration,
  native memory measurement, atomic raw-telemetry publication, bounded event
  records, cancellation, and explicit failures.
- Host alias selection, live XML parsing, compatibility attestation,
  freshness-qualified telemetry, allocation-neutral checks, host/device
  headroom, and fail-closed actuation.
- Absolute desired-target calculation and durable desired/requested/current
  reconciliation with command journaling, no-replay recovery, partial-progress
  accounting, and latching.
- Rust `xtask` workflows for local and cross-platform gates, host prerequisites,
  QGA health, live resize, Windows verification, and detached qualification.
- Version-1 shared-core `HostPoolPolicy`, `GuestDemandReport`, and
  `HostPoolPlan` contracts with checked total-RAM/device conversion and a
  deterministic side-effect-free arbiter.

Legacy mode directly uses available physical memory and commit headroom plus
configured reserves. The in-progress WN4 growth mode instead uses the
commit-centred pressure requirement and lets authoritative low-memory state
select its configured urgent step. Standby/free/modified lists, paging
activity, hard faults, and compression still do not influence the applied
target.

## Known gaps

- Windows-native notification collection and host shadow comparison are
  implemented, but semantic workload evidence, pressure-aware actuation, and
  recovery/endurance evidence are not qualified.
- Windows does not expose a general recommended-RAM byte target for KVM; the
  host's minimal mapping from pressure evidence to bytes still requires shadow
  calibration and validation.
- There is no final host-wide deployment configuration syntax or runtime
  loader for the implemented semantic pool/member policy.
- Pool accounting does not yet include a durable growth reservation ledger or
  restart reconstruction around the HPM0 base-plus-live-current charge.
- The pure `global_pool` planner has no exclusive coordinator, Windows provider
  adapter, durable dispatch authority, or runtime integration.
- Concurrent demand, reclaim-before-transfer, mixed guest-OS providers, and
  multi-VM recovery/endurance remain unimplemented and unqualified.
- Classic Windows Event Log text rendering still needs a packaged message
  resource; structured EventData remains available.
- Direct per-VM capacity selection, independent reclaim, fixed-headroom demand,
  threshold demand, and temporary schema fallback still require deletion at
  their named WN/HPM retirement gates.

## Evidence policy

Current run evidence lives under the run directory selected by `cargo xtask`,
with exact deployment facts in the deployment manifest. This status file does
not embed VM-specific values, historical hashes, test counts, paths, timeouts,
or workload profiles. Completion claims must name the relevant task and
evidence location.

See [BACKLOG.md](BACKLOG.md), [docs/roadmap.md](docs/roadmap.md),
[docs/QA-roadmap.md](docs/QA-roadmap.md), and
[docs/testing.md](docs/testing.md).
