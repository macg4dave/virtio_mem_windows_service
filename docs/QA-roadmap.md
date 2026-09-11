# Unattended single-controller QA roadmap

## Scope and current decision

This roadmap covers one trusted development guest, one explicitly configured
virtio-mem device, and one host controller. The exact VM, alias, service
identities, paths, artifacts, and configuration belong in the current
deployment manifest and qualification evidence, not in this reusable plan.

These gates provide the execution detail for AR1 through AR4 in the
[automatic-resizing release roadmap](roadmap.md). Passing them is the required
single-VM checkpoint before global-controller work can cross a live boundary;
it is not the final multi-VM release decision.

The current decision is **NO-GO for unattended automatic resizing**. The host
controller is in a fail-stop safe hold while candidate deployment coherence,
production telemetry transport, current attestation, applied workload cycles,
recovery, and endurance evidence remain incomplete. See
`qa-deployment-manifest.md` for the current inspected deployment delta.

Multi-VM arbitration, untrusted guests, production support, and direct driver
control are outside this gate.

## Safety invariants

1. Windows publishes measurements only; the host owns policy and actuation.
2. Alias-scoped live libvirt `current` is allocation authority.
3. Ordinary actuation is prohibited while requested and current differ.
4. Every request is aligned, bounded by current configuration, headroom
   checked, freshly attested, journaled, and resolved by live reread.
5. Missing, stale, replayed, malformed, cross-VM, or provenance-free telemetry
   cannot authorize reclaim.
6. Ambiguous or stalled actuation latches durably and is never blindly retried.
7. Automatic reclaim remains default-on; a reviewed explicit pause is allowed.
8. A live run records initial state, explicit time bounds, cleanup, recovery,
   and final state before mutation.
9. Only one controller/device is active in this qualification scope.

## Gates

| Gate | Exit condition |
| --- | --- |
| QA-G0 Safe hold | Unhealthy or incoherent deployment cannot mutate; guest and device health are recorded |
| QA-G1 Coherent deployment | Current hashed artifacts, configuration, identities, ACLs, attestation, and production telemetry transport agree |
| QA-G2 Automatic actuation | Explicit resident and committed qualification configurations prove growth, falling demand, safe reclaim/constrained handling, and renewed pressure |
| QA-G3 Recovery | Telemetry loss, service interruption, command ambiguity, cancellation, and partial/no progress produce bounded actionable states without replay or overlap |
| QA-G4 Endurance | A reviewed run plan exercises enough cycles and idle time to cover the stated risk model, with continuous evidence and no unexplained transition |
| QA-G5 Decision | Support profile, operator response, evidence index, and residual risks are reviewed for an explicit GO or NO-GO |

## Task board

### Baseline and deployment

| ID | Task | Status |
| --- | --- | --- |
| QA-T001 | Establish fail-stop safe hold and confirm unchanged guest/device health | Complete |
| QA-T002 | Record installed/candidate hashes, units, configuration, identities, and deployment delta | Complete |
| QA-T003 | Correct acknowledged durability and SCM identity/registration defects | Complete |
| QA-T004 | Implement the least-privilege production telemetry transport | Complete |
| QA-T005 | Install and verify the current Windows candidate, configuration, ACLs, telemetry, and lifecycle | Complete |
| QA-T006 | Install and verify the current host candidate, instance configuration, and fail-stop service state | In progress |
| QA-T007 | Calibrate deployment-specific visible base and regenerate/review compatibility attestation | Prepared; blocked by QA-T006 install |
| QA-T008 | Prove telemetry handoff and replay/session behavior across independent service restarts without actuation | Blocked by QA-T005–QA-T007 |

### Automatic behavior and recovery

| ID | Task | Status |
| --- | --- | --- |
| QA-T009 | Run an explicitly configured resident-memory qualification through the production path | Blocked by QA-T008 |
| QA-T010 | Run an explicitly configured committed-memory qualification through the same path | Blocked by QA-T009 |
| QA-T011 | Prove renewed pressure during an owned pending shrink without lower-request overlap | Blocked by QA-T009 |
| QA-T012 | Record an explicit cleanup/recovery result for the captured initial target | Blocked by QA-T009 |
| QA-T013 | Exercise stale, missing, malformed, and replayed telemetry in converged and pending states | Blocked by QA-T009 |
| QA-T014 | Exercise host-controller interruption across converged, active, and latched states | Blocked by QA-T009 |
| QA-T015 | Exercise Windows service interruption and session rollover | Blocked by QA-T010 |
| QA-T016 | Exercise pre-command rejection and ambiguous command outcome | Blocked by QA-T013–QA-T014 |
| QA-T017 | Exercise zero-progress and partial-progress shrink recovery | Blocked by QA-T011–QA-T012 |
| QA-T018 | Verify observability and operator response for every terminal state | Blocked by QA-T013–QA-T017 |

### Endurance and decision

| ID | Task | Status |
| --- | --- | --- |
| QA-T019 | Define and run a repeated-cycle plan whose cycle count, workload sizes, timings, and acceptance criteria are justified by the current risk model | Blocked by QA-T018 |
| QA-T020 | Define and run an unattended soak whose duration and schedule are explicit inputs justified by the failure modes under review | Blocked by QA-T019 |
| QA-T021 | Classify every warning, restart, dropped sample, non-convergence, attestation failure, and operator action | Blocked by QA-T020 |
| QA-T022 | Publish the development support profile and known limitations | Blocked by QA-T021 |
| QA-T023 | Publish the operator health, pause, recovery, upgrade, and rollback runbook using current `xtask`/product commands | Blocked by QA-T018 and QA-T021 |
| QA-T024 | Record the final reviewed GO or NO-GO decision and evidence index | Blocked by QA-T022–QA-T023 |

## Qualification rules

Use `cargo xtask qualification` for automatic-controller workload runs. Every
run supplies its sizes, safety cap, timings, services, endpoints, paths,
sampling, and acceptance deltas explicitly and stores the versioned
configuration with its evidence. Do not promote a prior run's values into a
new profile.

Use `cargo xtask live-resize` only for a separately scoped reversible manual
test. It is not a substitute for observing the automatic controller through
the production telemetry path.

Endurance duration and cycle count are selected only after the failure modes,
observation frequency, and confidence target are written down. A short smoke
run cannot satisfy a longer plan, but this roadmap does not encode an arbitrary
universal duration.

## Final decision checklist

- [ ] Current hashed Windows and host artifacts are installed.
- [ ] Production raw telemetry and least-privilege transport/ACLs are verified.
- [ ] Calibration and attestation match fresh live state.
- [ ] The device starts converged and exactly one controller is active.
- [ ] Explicit resident and committed apply runs meet their recorded criteria.
- [ ] Growth, falling demand, reclaim/constrained handling, and renewed
      pressure are correctly observed.
- [ ] Loss, interruption, ambiguity, and partial/no-progress recovery gates pass.
- [ ] No request is replayed or overlapped.
- [ ] Health, last success, restart count, latch reason, and resize events are
      monitorable against documented alert criteria.
- [ ] The reviewed repeated-cycle and soak plans pass.
- [ ] Final state and every warning/operator action are explained.
- [ ] Pause, recovery, upgrade, and rollback procedures are rehearsed.

Any unchecked item keeps the result at NO-GO. Store raw evidence under the
ignored artifact hierarchy and summarize only durable outcomes in project
status and task documents.
