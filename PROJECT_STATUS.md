# Project Status

Updated: 2026-09-11

## Summary

The project is in single-VM deployment and qualification. Native Windows
telemetry, the host-side allocation join, compatibility attestation,
quantitative targets, durable reconciliation, and bounded recovery are
implemented. The current installed stack is not treated as a coherent
candidate, so the host controller remains held and the development deployment
is NO-GO.

## Current work

- QA-T002 records the exact installed deployment in
  [qa-deployment-manifest.md](docs/qa-deployment-manifest.md).
- TASK-009 deploys and verifies Windows configuration, protected telemetry,
  and service-account publication.
- TASK-028 uses the detached `cargo xtask qualification` workflow to qualify
  the controller and platform with run-specific parameters.

The next authorized mutation is selected only after the manifest and current
preflight show a coherent candidate and a reviewed rollback path.

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

## Known gaps

- The installed host and Windows components have not yet been proven to share
  one current configuration and telemetry contract.
- Installed Windows configuration/ACL publication evidence is incomplete.
- Compatibility attestation must be recreated after coherent deployment.
- Applied workload, reclaim, renewed-pressure, fault, restart, and endurance
  evidence is incomplete.
- Classic Windows Event Log text rendering still needs a packaged message
  resource; structured EventData remains available.
- Multi-controller actuation is deferred until M11 provides atomic host-pool
  reservation.

## Evidence policy

Current run evidence lives under the run directory selected by `cargo xtask`,
with exact deployment facts in the deployment manifest. This status file does
not embed VM-specific values, historical hashes, test counts, paths, timeouts,
or workload profiles. Completion claims must name the relevant task and
evidence location.

See [BACKLOG.md](BACKLOG.md), [docs/QA-roadmap.md](docs/QA-roadmap.md), and
[docs/testing.md](docs/testing.md).
