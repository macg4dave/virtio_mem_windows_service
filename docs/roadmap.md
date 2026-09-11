# Product Roadmap

## Current position

The single-VM architecture is implemented through quantitative target
calculation and durable desired/requested/current reconciliation. Current work
is deployment coherence and live qualification, not another controller design.
The development target remains NO-GO until the ordered gates in
[QA-roadmap.md](QA-roadmap.md) pass.

`BACKLOG.md` owns task state. This roadmap describes dependencies and intended
outcomes; it does not duplicate operational commands, test profiles, or prior
machine observations.

## Milestones

| Milestone | Status | Outcome |
| --- | --- | --- |
| M0-M8 | Complete | Repository, Windows service, native telemetry, lifecycle, configuration, and read-only guest/host validation foundations. |
| M9-M9e | Complete | Alias-scoped host adapter, compatibility attestation, systemd runtime, Rust operations, and freshness-correct host telemetry. |
| M10a-M10d | Complete in code | Allocation authority, optional diagnostic boundary, host-side allocation join, bounded raw telemetry, replay protection, and ACL provisioning. Installed deployment evidence remains part of QA. |
| M10e | Complete | Quantitative desired allocation with checked bounds, history, safe floor, and restart-safe policy state. |
| M10f | Complete | Durable desired/requested/current reconciliation, command ownership, ambiguity handling, and latching. |
| M10g | In progress | Coherent deployment and single-VM workload/controller/platform qualification using durable run evidence. |
| M11 | Planned | Hermetic global-pool accounting and arbitration over multiple absolute targets. |
| M11a | Planned | Controlled target-based reclaim after global reservation is proven. |
| M12 | Planned | Operational observability, recovery, and failure injection. |
| M13 | Planned | Repeatable deployment, monitoring, rollback, and release readiness. |

## Dependency path

M10g must finish before live multi-target work begins. M11 first proves atomic
host-pool accounting in simulation. M11a then adds controlled reclaim. M12 and
M13 harden and operationalize only behavior already proven by those gates.

## Architecture outcomes to preserve

- Windows provides fresh, identity-bound native telemetry and never issues a
  host resize.
- The host joins that telemetry with the selected live libvirt device state.
- Every mutation requires current compatibility, headroom, freshness,
  alignment, ownership, and convergence checks.
- Desired demand, device request, observed current allocation, and control
  health remain distinct.
- A command with an uncertain result is observed and reconciled; it is never
  blindly replayed.
- Product policy defaults are configurable and documented once in
  [target-controller.md](target-controller.md).
- Live-test timings, targets, thresholds, and workload profiles are explicit
  run inputs recorded with their evidence.

## Release gate

Release readiness requires the current candidate to pass focused tests, the
local aggregate gate, the native Windows gate, coherent deployment checks,
bounded live qualification, fault/recovery qualification, and an explicitly
configured endurance run. A previous run, test count, binary hash, VM name, or
timeout is never carried forward as a default.
