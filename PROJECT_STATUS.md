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
recovery, and a pure non-actuating pool-planner prototype are implemented. The
fixed-headroom formula is a temporary migration baseline, not a supported
long-term policy. The completed coherent deployment evidence remains valid.
The controller remains disabled/inactive, and automatic resizing remains
NO-GO until the pressure-aware provider and host-pool paths are implemented and
qualified.

## Current work

- TASK-035 completed the implementation/document audit, Microsoft-native signal
  research, target architecture, obsolete-item classification, and WN0 cleanup.
- TASK-038 / WN3 remains the claimed implementation slice: qualify the
  implemented host persistence, version-2 status, and append-only comparison
  path. Explicit shadow mode joins accepted telemetry to live allocation and
  always suppresses actuation. The qualification workflow now has an explicit
  no-resize shadow contract and retains protected comparisons.
  The new focused, aggregate local, and native Windows gates pass. The shadow
  candidate is installed with matching hashes while the unit remains
  disabled/inactive. Resident schema-v2 fallback qualification subsequently
  passed with 119 samples, zero warnings, unchanged allocation, and retained
  unavailable/blocked comparisons. The native-gated schema-v3 Windows service
  is now installed with protected ACLs and advancing telemetry; notification
  workload correlation remains.
- TASK-028 and QA-T009 are paused as fixed-headroom qualification. Their
  evidence remains useful for reconciliation and platform behavior but cannot
  qualify the replacement demand policy.
- The roadmap now preserves WN3 as the current implementation slice, treats
  WN0-WN10 as Windows-provider/per-VM component qualification, and makes
  HPM0-HPM6 the mandatory system path to a final release decision.

The current slice changes host/shared-core code and documentation. Deployment
and live mutation have not yet been performed.

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

- Windows-native notification collection and host shadow comparison are
  implemented, but semantic workload evidence, pressure-aware actuation, and
  recovery/endurance evidence are not qualified.
- Windows does not expose a general recommended-RAM byte target for KVM; the
  host's minimal mapping from pressure evidence to bytes still requires shadow
  calibration and validation.
- There is no host-wide configuration for total VM-pool bytes, members,
  minimums, priorities, or provider kinds.
- Existing pool inputs do not yet charge each VM's host-derived non-reclaimable
  base RAM alongside live virtio-mem allocation and reservations.
- The pure `global_pool` planner has no durable reservation ledger, exclusive
  coordinator, provider-neutral demand-report input, or runtime integration.
- Its current separate growth/reclaim priorities and host-pressure-driven
  reclaim do not implement the contention-only, named-recipient transfer model.
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
