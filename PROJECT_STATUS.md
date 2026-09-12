# Project Status

Updated: 2026-09-13

## Summary

The project has redirected its single-VM controller toward Windows-native
memory pressure. Basic Windows telemetry, the host-side allocation join,
compatibility attestation, fixed-headroom targets, durable reconciliation, and
bounded recovery are implemented. The target formula is now a compatibility
baseline rather than the intended release policy. The completed coherent
deployment evidence remains valid. The controller remains disabled/inactive,
and automatic resizing remains NO-GO until the new pressure-aware path is
implemented and qualified.

## Current work

- TASK-035 completed the implementation/document audit, Microsoft-native signal
  research, target architecture, obsolete-item classification, and WN0 cleanup.
- TASK-036 / WN1 is next: define and test the additive telemetry, capability,
  warm-up, and fallback contract without changing collection or actuation.
- TASK-028 and QA-T009 are paused as fixed-headroom qualification. Their
  evidence remains useful for reconciliation and platform behavior but cannot
  qualify the replacement demand policy.

No live mutation was performed for the current source change.

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

The audit found that the active raw-path formula directly uses only available
physical memory and commit headroom plus configured reserves. Low/high memory
notifications, standby/free/modified lists, paging activity, hard faults, and
compression do not currently influence `desired`.

## Known gaps

- Windows-native pressure collection, semantic workload evidence, shadow target
  comparison, pressure-aware actuation, and its recovery/endurance evidence are
  not implemented or qualified.
- Windows does not expose a general recommended-RAM byte target for KVM; the
  host's minimal mapping from pressure evidence to bytes still requires shadow
  calibration and validation.
- Classic Windows Event Log text rendering still needs a packaged message
  resource; structured EventData remains available.
- Multi-controller actuation is deferred until WN10's release scope has durable
  atomic host-pool reservation and live multi-guest qualification.

## Evidence policy

Current run evidence lives under the run directory selected by `cargo xtask`,
with exact deployment facts in the deployment manifest. This status file does
not embed VM-specific values, historical hashes, test counts, paths, timeouts,
or workload profiles. Completion claims must name the relevant task and
evidence location.

See [BACKLOG.md](BACKLOG.md), [docs/roadmap.md](docs/roadmap.md),
[docs/QA-roadmap.md](docs/QA-roadmap.md), and
[docs/testing.md](docs/testing.md).
