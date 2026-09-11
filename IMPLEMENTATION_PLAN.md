# Implementation Plan

`BACKLOG.md` owns execution order. This document describes the remaining
technical dependency chain without prescribing machine-specific commands or
test values.

## Complete the single-VM deployment

1. Capture the installed host and Windows deployment with exact identities,
   paths, configuration, hashes, security state, and current health.
2. Build one candidate through the maintained `cargo xtask` gates.
3. Deploy matching Windows and host artifacts with required configuration and
   a reviewed rollback path.
4. Recreate compatibility attestation from the deployed allocation-neutral
   state.
5. Run the no-actuation preflight and verify telemetry freshness, service
   identity, controller ownership, headroom, and convergence.

All settings that influence operations are explicit deployment or run inputs.
Do not copy values from an earlier manifest or test.

## Qualify the single-VM controller

Use `cargo xtask qualification` as the sole maintained workload/controller
qualification entry point. Define the target, workload mode, allocations,
phase durations, sampling, command deadline, and acceptance thresholds for the
current run. Preserve its versioned status, events, metrics, journals, and
review result.

Qualification must separate:

- demand calculation from device progress;
- desired, requested, and current allocation;
- workload release from actual platform reclaim;
- preparation rejection from uncertain command outcome;
- safe latching from service failure;
- cleanup from any separately authorized baseline restoration.

## Build the global controller

After single-VM qualification:

1. Model actual allocations, absolute desired targets, per-VM safe floors,
   host reserve, and pool-free capacity in deterministic simulation.
2. Add atomic reservation before any multi-target actuation.
3. Exercise stale input, competing growth, reclaim priority, partial progress,
   cancellation, restart, and command ambiguity.
4. Introduce live multi-target actuation only after simulation and single-VM
   safety gates pass.

## Operationalize

Add structured health, metrics, fault injection, repeatable deployment,
rollback, and monitoring around the proven controller. Endurance duration and
cycle count are chosen explicitly for each release candidate and recorded in
the run manifest; they are not repository defaults.

## Validation rule

Every change receives focused tests first, then the relevant `cargo xtask`
gate. Live testing requires the explicit run configuration described in
[docs/testing.md](docs/testing.md). Historical evidence can explain a risk but
cannot authorize a command or supply a current value.
