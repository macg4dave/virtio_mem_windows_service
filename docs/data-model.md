# Data Model

All external memory quantities are checked unsigned byte counts. Display units
are presentation only and must be converted explicitly at a boundary.

## Windows raw telemetry

`RawTelemetryEnvelope` schema v3 contains:

- schema version;
- configured VM and service identity;
- process session identity and session-local sequence;
- wall-clock and monotonic observation time;
- native physical and commit counters;
- a versioned `windows_native` capability and signal extension; and
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

The extension groups paired memory-resource notification state, reusable and
modified memory-list bytes, and optional paging-rate samples. Each fixed-size
group declares its capability and uses an explicit `supported`, `unavailable`,
`failed`, or `warming` observation; absence and warm-up are never encoded as a
zero measurement. Every signal state carries its own monotonic observation
time. Paging rates use a non-zero sampling interval and milli-events/second
integer units so the wire representation is deterministic.

Schema v2 without the extension remains decodable as
`legacy_v2_fallback`. Schema v3 without the extension, schema v2 with it,
unknown fields, capability/status contradictions, invalid warm-up progress,
future signal timestamps, inconsistent memory-list bounds, and unsupported
versions are invalid evidence. Capabilities cannot change within a producer
session. A new sequence-zero session may renegotiate them. Fallback and
incomplete observations cannot provide pressure-qualified reclaim evidence.
These additions remain measurements and never contain a desired target. See
[windows-native-pressure-controller.md](windows-native-pressure-controller.md).

That schema-v2 decoder is a bounded producer/consumer migration path, not an
indefinite compatibility promise. WN9 removes it after every supported deployed
producer uses the current schema and the declared rollback window closes.

WN2 populates only `memory_resource_notifications`. Its capability is
`supported` on the Windows build even when a handle creation or query attempt
reports `failed`; this distinguishes API availability from an observation
failure. Low and high handles are queried as one categorical sample. Neither
signalled is neutral, while both signalled is contradictory invalid-data
evidence. The other two capability groups remain explicitly unavailable.

## Target policy

The host joins accepted Windows telemetry with the selected live virtio-mem
device:

- `current`: authoritative allocation exposed to the guest;
- `requested`: device target currently being pursued;
- `demand_target`: the provider adapter's bounded total-guest-RAM request before
  host-wide arbitration;
- `pool_grant`: the total guest RAM granted to a member by the host pool;
- `desired`: the alias-scoped device target derived from `pool_grant` that the
  per-VM reconciler may pursue;
- `safe_floor`: lowest target permitted by qualified recent demand;
- `effective_max`: configured/device upper bound after safety headroom.

The current single-VM implementation still calculates device-scoped `desired`
directly. This is incremental qualification scaffolding: after host-pool
integration, a provider produces total-RAM `demand_target`, the pool manager
produces total-RAM `pool_grant`, the host converts it to device-scoped `desired`,
and no per-VM path may bypass that grant.

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

WN3 inserts a versioned `WindowsPressureAssessment` beside the fixed-headroom
estimator. It keeps committed-memory requirement, notification-led pressure
(`low_memory`, `neutral`, `high_memory`, or `unavailable`), and shrink safety
as separate results with stable reasons and input identity. Its history and
latest result are checkpointed under a separate policy fingerprint, and each
comparison is appended as versioned JSON-lines evidence. In shadow mode it
always produces `NoChange`; it is not allocation authority and does not remove
`desired`, `requested`, or `current` from the model.

## Guest demand provider

The host-pool boundary consumes version-1 OS-neutral `GuestDemandReport`, not
a Windows raw-telemetry record. A provider adapter derives it from evidence
native to one guest operating system. The report contains:

- exact VM/device and provider kind/instance identity, report and provider
  policy versions, observation time, and availability/continuity state;
- a bounded quantitative total-RAM `demand_target` and effective maximum;
- pressure/urgency and an explainable reason set;
- shrink eligibility and a qualified total-RAM `safe_floor`; and
- explicit unavailable, warming, invalid, or degraded state.

The report contains neither pool capacity nor a granted target. Windows uses
`WindowsPressureAssessment` as its adapter input. A future Linux provider may
use Linux-native pressure and memory evidence without copying the Windows raw
schema or algorithms. Provider-specific evidence stays behind the adapter;
arbitration sees one common demand contract.

## Host RAM pool

`HostPoolPolicy` version 1 is the implemented semantic host-wide policy. It has
an arbitration version, a total VM-pool byte limit, and an explicit member set.
Each member binds exact VM/device identity to total-RAM minimum and maximum
bounds, one non-zero contention priority, provider kind, and a positive report
freshness bound. The Rust contract is serializable for deterministic fixtures;
the final deployment file syntax and provider-specific configuration remain
deferred.

Pool and member bounds are total guest-RAM bytes. For each member:

- `base_allocation`: host-derived non-reclaimable RAM outside the selected
  virtio-mem device;
- `current`: authoritative live allocation contributed by that device;
- `pool_charge`: `base_allocation + current` plus any outstanding growth
  reservation not already visible in `current`; and
- `minimum`/`maximum`: total guest-RAM bounds converted to device targets only
  at the reconciler boundary.

These values are checked and geometry-aware. Windows/Linux totals may validate
topology but never replace host allocation authority. QEMU overhead and other
host use remain covered by host safety reserves rather than being presented as
guest RAM.

The sum of geometry-valid total-RAM minimum guarantees must fit within the
pool. An `enabled` member requires settled live allocation and a fresh usable
report. An `inactive` member cannot move and retains the greater of its current
charge and minimum guarantee. An `unavailable` member retains that charge and
blocks movement for the complete snapshot. A `removed` member remains held and
charged until a later explicit policy update removes its drained identity; it
is never silently omitted. Priority does not reserve discretionary capacity or
define a permanent share. When all eligible growth fits, each VM may grow
toward demand regardless of priority. Priority ordering and exact identity as
the same-priority tie-break become active only when eligible requests exceed
currently free capacity.

One exact full-member arbitration snapshot joins every fresh
`GuestDemandReport` with host-derived non-reclaimable base, device geometry,
live `requested`, and authoritative `current`. Version-1 `HostPoolPlan` records,
per VM:

- assessed demand and safe floor;
- current allocation, requested state, and explicit snapshot blockers for
  in-flight allocation or unavailable member/provider evidence;
- total-RAM pool grant, derived device target, and
  hold/grow/reclaim/constrained reason;
- unmet demand and, when applicable, the higher-priority recipient and strictly
  lower-priority donor dependency;
- non-durable growth grants or reclaim dependencies awaiting later reservation
  and observation; and
- the pool totals before and after the plan.

HPM0 plans have no runtime authority and cannot be dispatched. The HPM1 durable
reservation ledger will bind the complete member/configuration fingerprint,
plan generation, per-VM grant, command ownership, and observed total-RAM
allocation. A growth grant consumes pool capacity before dispatch. A reclaim
plan does not create free capacity until live `current` confirms the decrease.
Restart reconstructs the same accounting from the ledger and fresh live state;
it never lets separate per-VM services recalculate the same free bytes.

When aggregate eligible growth fits in free pool capacity, the arbiter grants
it without consulting priority. When requests contend for insufficient free
capacity, priority orders use of the remainder. A still-unmet higher-priority
request may select only a strictly lower-priority donor that is underutilised,
shrink-eligible, and above `max(minimum, safe_floor)`. Equal- or
higher-priority members are not donors for that request. Underutilisation
without contention causes no priority reclaim. If enough safe capacity cannot
be released, remaining demand is reported as waiting/constrained rather than
violating a guarantee or inventing capacity.

## Controller status

`ControllerStatusSnapshot` version 2 is a bounded read-only operational view.
It joins a fresh alias-selected `VirtioMemState` with the matching policy
checkpoint and exposes:

- device size/block geometry and desired, safe-floor, effective-maximum,
  requested, and current byte counts;
- the accepted telemetry session, sequence, monotonic time, and wall-clock
  time, or explicit cold state;
- history readiness and reclaim state (`cold`, `warming`, `ready`, `paused`,
  `blocked_in_flight`, `blocked_latched`, or `blocked_recovery`);
- capacity state (`unknown`, `available`, or `at_effective_maximum`);
- command ownership and immutable command details when an intent exists; and
- control health, latch/recovery reasons, fingerprints, and the last reviewed
  latch-clear reason; and
- the pressure-policy mode plus, in shadow mode, its policy fingerprint,
  history-entry count, and latest assessment or cold state.

The snapshot is not persisted by the status command and is not control state.
Malformed, oversized, newer-version, contradictory, unaligned, or
identity/fingerprint-mismatched inputs fail closed.

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

The current host reads one explicit VM, device alias, memory bounds, thresholds,
durations, source modes, telemetry identity/path, policy-state path, attestation
path, transport-specific host acknowledgement path, host reserve, resize
quanta, policy reserves, history, and hysteresis. Only the maximum sample gap
may be derived from the polling interval. Deployment examples use placeholders
rather than machine values.

The replacement host-wide configuration adds the pool byte limit, explicit
membership, per-VM minimum/maximum/priority/provider, arbitration policy, and
durable pool-ledger path. Per-instance policy files cease to own allocation
once their member has moved behind the coordinator.

Pressure-aware settings require a versioned migration. Existing fixed
physical/commit reserves may remain only for a named qualification or rollback
window. After the replacement pressure assessment is qualified, remove their
primary-policy and compatibility paths rather than silently changing meaning
or supporting both algorithms indefinitely.
