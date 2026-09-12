# Data Model

All external memory quantities are checked unsigned byte counts. Display units
are presentation only and must be converted explicitly at a boundary.

## Windows raw telemetry

`RawTelemetryEnvelope` contains:

- schema version;
- configured VM and service identity;
- process session identity and session-local sequence;
- wall-clock and monotonic observation time;
- native physical and commit counters;
- explicit measurement and allocation-provenance labels.

It contains no libvirt allocation and no resize recommendation. The publisher
atomically replaces the current record and retains bounded predecessors. The
host validates identity, ordering, freshness, counter consistency, file bounds,
and durable replay acknowledgement before accepting a sample.

The configured raw transport is explicit. `qga-file` reads the protected
Windows current record through standard bounded guest-file operations and
keeps acknowledgement state at a separate host path. `file` reads an
explicitly provisioned host-local record. Transport success alone never
acknowledges or authorizes a sample.

Telemetry read failures distinguish unavailable transport from invalid
evidence. Neither produces a sample or authorizes actuation. Invalid evidence
clears estimator history; a transport interruption leaves accepted history
unchanged so the next accepted sample's time gap determines whether the
history remains qualified.

The planned pressure-aware schema revision adds low/high memory-resource
notification state, reusable and modified memory-list bytes, paging-rate
samples with their interval/warm-up state, and per-signal availability or
error status. These additions remain measurements. Unsupported is distinct
from zero, and a partial record cannot silently authorize reclaim. See
[windows-native-pressure-controller.md](windows-native-pressure-controller.md).

## Target policy

The host joins accepted Windows telemetry with the selected live virtio-mem
device:

- `current`: authoritative allocation exposed to the guest;
- `requested`: device target currently being pursued;
- `desired`: stable absolute policy target;
- `safe_floor`: lowest target permitted by qualified recent demand;
- `effective_max`: configured/device upper bound after safety headroom.

Control health is separate from those values. It records convergence, growth,
shrink, constrained progress, uncertain command outcome, recovery-required
state, or a durable latch. Elapsed time can change health but cannot change
measured demand.

Estimator history stores calculated demand candidates and their identities,
not allocations. Its versioned checkpoint is bounded, durably flushed,
fingerprint-bound, and atomically replaced. Missing estimator state restarts
conservatively; malformed control state fails closed.

The exact formulas and required policy inputs live only in
[target-controller.md](target-controller.md).

The replacement design inserts a versioned `WindowsPressureAssessment` between
accepted raw telemetry and the estimator. It records an explainable class
(`urgent_grow`, `grow_or_hold`, `neutral_hold`, `reclaim_eligible`, or
`unavailable`) plus contributing evidence. It is not allocation authority and
does not remove `desired`, `requested`, or `current` from the model.

## Live device state

The alias-selected live XML record contains device `size`, `block`,
`requested`, and `current`. All are validated for bounds and alignment.
`requested != current` is an asynchronous operation, not success or failure by
itself.

Compatibility attestation binds allocation-neutral live domain/QEMU evidence,
selected QOM properties, versions, target identity, driver review, workload
exclusions, and capacity review. Allocation movement is normalized; topology,
identity, configuration, or reviewed compatibility drift revokes it.

Balloon `dommemstat` values are freshness-qualified demand context. They are
not virtio-mem allocation authority. QGA is maintained as health/identity
evidence.

## Command ownership

Before mutation, the reconciler durably records operation identity, direction,
prior live state, target, telemetry identity, and policy/compatibility
fingerprints. It immediately rereads live state after every command result.

A visible target is observed to convergence. An unchanged prior state or any
other ambiguous result latches actuation for explicit review. Restart can
resume observation but cannot replay the command.

## Workload and qualification evidence

`virtio-mem-workload` receives explicit mode, allocation, hold, cap, refresh,
and identity values. Its versioned JSON-lines records describe declared
baseline, peak, settled, renewed, and complete phases. Touched pages distinguish
the resident scenario from committed-only allocation; they are not an exact
working-set measurement.

`cargo xtask qualification` records its complete immutable run configuration,
atomic status and result records, append-only events, workload evidence, host
metrics, controller journal, and logs in a unique run directory. Its thresholds
and durations are run data, not schema defaults. Workload records and Windows
telemetry remain evidence only; neither gains resize authority.

The qualification result derives separate initial-growth, settled-reclaim, and
renewed-growth measurements from phase-correlated live `requested`/`current`
samples. It also records whether renewed pressure began while the last settled
sample showed a pending shrink, any observed lower request before prior
convergence, and telemetry session/sequence discontinuities. These fields are
evidence classification only and never drive product actuation.

## Windows configuration

The Windows service requires a versioned JSON document containing service
identity, VM identity, polling and shutdown durations, paths, and report
settings. Missing, malformed, unsupported, or invalid
configuration is a visible startup failure. There is no operational fallback
profile.

## Host configuration

The host reads an explicit VM, device alias, memory bounds, thresholds,
durations, source modes, telemetry identity/path, policy-state path, attestation
path, transport-specific host acknowledgement path, host reserve, resize
quanta, policy reserves, history, and hysteresis.
Only the maximum sample gap may be derived from the polling interval.
Deployment examples use placeholders rather than machine values.

Pressure-aware settings require a versioned migration. Existing fixed
physical/commit reserves become fallback safety inputs rather than silently
changing meaning in place.
