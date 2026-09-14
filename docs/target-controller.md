# Quantitative Target Controller Contract

## Status and scope

This is the normative contract for per-VM demand assessment, asynchronous
virtio-mem reconciliation, and qualification. The current implementation
replaces directional demand estimates with an explicit desired target. In the
destination architecture, the provider assessment produces a total-RAM
`demand_target`, the host-wide pool manager issues a total-RAM `pool_grant`, and
this reconciler pursues only the derived device-scoped `desired`.

The project direction has changed. The fixed-reserve estimator documented
below describes the currently implemented temporary migration baseline, not
the intended release sizing algorithm. Its replacement will use Windows-native
memory-resource notifications, commit evidence, reusable-memory lists, and
paging evidence as described in
[windows-native-pressure-controller.md](windows-native-pressure-controller.md).
Until that replacement is implemented and qualified, the installed controller
remains disabled outside explicitly bounded tests, and fixed reserves must not
be presented as a Windows pressure prediction.

Automatic Windows shrink is currently a default-on product capability. A deployment may
set `VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK=false` for diagnosis or a deliberate
rollout pause, but absence of that setting means enabled. Enabled reclaim still
requires fresh continuous telemetry, a warmed history window, a valid safe
floor, compatible device geometry, current compatibility attestation, and an
unlatched reconciler. Same-target re-notification remains a separate default-
off diagnostic feature and is not part of normal target reconciliation.

After HPM4, default-on means the host-pool manager may execute qualified
reclaim when unmet higher-priority demand requires a transfer. It does not mean
that a per-VM loop proactively shrinks an underutilised VM when the pool is
unconstrained. The current direct single-VM shrink path is qualification
scaffolding and is removed after pool-owned reclaim qualifies.

This contract remains single-VM until the host RAM pool manager supplies atomic
reservation. Its single-VM target wiring is qualification scaffolding, not a
second long-term capacity authority. Windows publishes measurements only; the
RHEL host owns assessment, pool allocation, host safety, reconciliation, and
actuation.

## Authoritative inputs and terminology

All quantities are checked `u64` byte counts at external boundaries. Policy
arithmetic uses checked signed deltas so subtracting a reserve or baseline
cannot silently wrap.

- `C`: alias-scoped live libvirt `current`, the authoritative virtio-mem
  allocation.
- `Q`: alias-scoped live libvirt `requested`, the device's current target.
- `T`: Windows `physical_total_bytes`.
- `A`: Windows `physical_available_bytes`.
- `K`: Windows `commit_total_bytes`.
- `L`: Windows `commit_limit_bytes`.
- `B`: configured fixed visible base memory: the memory Windows reports when
  the selected virtio-mem device contributes zero current allocation. It is
  not inferred from QGA, balloon `actual`, aggregate host RAM, or the configured
  virtio-mem maximum.
- `demand_target`: the bounded total-guest-RAM request produced by the
  replacement guest-demand provider before host-wide arbitration.
- `pool_grant`: the total guest RAM granted by the host RAM pool manager.
- `desired`: after pool integration, the stable alias-scoped device target
  derived from `pool_grant` and consumed by this reconciler.
- `safe_floor`: the lowest target that pressure arbitration may consider from
  the current qualified history. It is not an actuation command.

`memory_load_percent`, `commit_peak_bytes`, cache, and kernel pool values remain
diagnostic and consistency evidence. `commit_peak_bytes` is a since-boot peak
and must not be used as rolling demand. Commit charge is a virtual-memory
constraint rather than a direct measure of resident physical use, so physical
and commit candidates are calculated separately and combined with `max`, never
added together.

## Implemented fixed-headroom estimator

### Geometry and cross-layer validation

Before accepting a sample, calculate `observed_base = T - C`. Reject underflow.
Require the absolute difference between `observed_base` and `B` to be no more
than `max(256 MiB, 2 * live_block_size)`. This tolerance absorbs stable Windows
visibility overhead without allowing a wrong VM, wrong device, or changed base
topology to drive policy. Compatibility review must confirm the tolerance for
the selected deployment from recorded baseline evidence.

The effective upper target is:

```text
effective_max = align_down(
    min(configured_maximum, device_size - 1 GiB),
    live_block_size
)
```

Fail closed if the subtraction is invalid or if the configured minimum exceeds
`effective_max`. This incorporates the existing device safety-headroom rule
before target construction instead of repeatedly producing an invalid target.

### Instantaneous candidates

Every deployment supplies its growth and reclaim quanta, physical and commit
reserves, safe-floor reserves, reclaim-history window, and downward hysteresis.
The maximum accepted sample gap may be supplied explicitly or derived from the
configured polling interval. The host exposes these settings:

- `VIRTIO_MEM_FIXED_VISIBLE_BASE_BYTES`;
- `VIRTIO_MEM_GROW_STEP_BYTES`;
- `VIRTIO_MEM_SHRINK_STEP_BYTES`;
- `VIRTIO_MEM_PHYSICAL_RESERVE_BYTES`;
- `VIRTIO_MEM_COMMIT_RESERVE_BYTES`;
- `VIRTIO_MEM_SAFE_FLOOR_PHYSICAL_RESERVE_BYTES`;
- `VIRTIO_MEM_SAFE_FLOOR_COMMIT_RESERVE_BYTES`;
- `VIRTIO_MEM_RECLAIM_HISTORY_SECONDS`;
- `VIRTIO_MEM_RECLAIM_MAX_GAP_SECONDS` (optional derived value);
- `VIRTIO_MEM_DOWNWARD_HYSTERESIS_BYTES`;
- `VIRTIO_MEM_POLICY_STATE_PATH` for the protected host checkpoint/journal.

WN3 adds a bounded migration selector. `VIRTIO_MEM_PRESSURE_POLICY_MODE` is
`legacy` when absent or explicitly selected. Selecting `shadow` requires
`VIRTIO_MEM_PRESSURE_MARGIN_RATIO_NUMERATOR`,
`VIRTIO_MEM_PRESSURE_MARGIN_RATIO_DENOMINATOR`,
`VIRTIO_MEM_PRESSURE_MARGIN_MINIMUM_BYTES`,
`VIRTIO_MEM_PRESSURE_MARGIN_MAXIMUM_BYTES`, and
`VIRTIO_MEM_PRESSURE_SHADOW_LOG_PATH`; none has a production numeric default.
The existing visible-base, history-window, maximum-gap, hysteresis, and live
geometry settings are shared inputs rather than copied shadow settings.

The fixed base and state path are required. A derived maximum-gap value is
materialized and validated at startup so a later poll-interval change cannot
silently alter an active history policy.

All reserves and hysteresis must be positive, block-alignable, ordered so floor
reserves do not exceed normal reserves, and configurable in canonical bytes.

These reserves are fixed policy margins. They are not derived from low-memory
notifications, paging activity, standby/cache pressure, hard faults, memory
compression, or another Windows memory-manager recommendation.

### WN3 shadow assessment

For every fresh accepted raw sample in `shadow` mode, the host first evaluates
the unchanged fixed-headroom estimator as the comparison baseline. It then
calculates committed demand plus the configured bounded proportional margin,
converts visible demand to a device-scoped requirement with the host-derived
visible base, and records independent pressure and shrink-safety results joined
to fresh alias-scoped `requested` and `current`.

Notification states are named `low_memory`, `neutral`, `high_memory`, and
`unavailable`. They qualify pressure but cannot invent requirement bytes.
Pressure history and the latest assessment are stored in the existing atomic
checkpoint under a separate pressure-policy fingerprint. A changed pressure
fingerprint clears only the shadow history/latest result; it does not discard a
valid legacy estimator or pending command record. Older checkpoints without
shadow state load with cold pressure history.

Each successful assessment is also appended as one versioned JSON line at the
configured shadow evidence path. Invalid evidence clears both estimator and
pressure history; transport unavailability preserves both until the next
accepted sample applies the maximum-gap rule. Shadow evaluation returns
`NoChange` regardless of either candidate, so it cannot create command intent
or reach the resize sink. Applied pressure-aware growth begins only in WN4.

For reserve pair `(Rphysical, Rcommit)`, calculate:

```text
physical_candidate = (T - A) + Rphysical - B
commit_headroom     = L - K
commit_candidate   = C + Rcommit - commit_headroom
candidate           = max(configured_minimum,
                          physical_candidate,
                          commit_candidate)
```

Negative candidates clamp to zero before the configured minimum is applied.
Overflow, `K > L`, or inconsistent counters reject the sample. The resulting
candidate is clamped to `effective_max` and aligned upward so rounding cannot
remove measured headroom. Clamping an otherwise valid demand candidate at the
effective maximum produces explicit `capacity_limited` health rather than
pretending the demand was fully satisfied.

Calculate `desired_now` with normal reserves and `floor_now` with safe-floor
reserves. Enforce:

```text
configured_minimum <= floor_now <= desired_now <= effective_max
```

### History, hysteresis, and durable desired

History stores timestamped calculated candidates, not allocations and not
pressure ratios. Growth is immediate: when `desired_now` exceeds durable
`desired`, raise `desired` to `desired_now` without waiting for history.

History and hysteresis delay downward movement; they do not make the fixed
headroom estimate predictive.

Downward movement requires a complete fresh configured history window with no
gap above the configured or derived maximum. Its candidate is the maximum
`desired_now` observed in that window. Lower durable `desired` only when that
maximum is at least the configured hysteresis below the previous desired value.
The reconciler, rather than the estimator, bounds how quickly allocation moves
toward the new absolute target.

`safe_floor` is the maximum `floor_now` in the same qualified window, clamped
not to exceed durable `desired`. Re-reading the same still-fresh atomic current
record is a normal no-new-sample state: it does not advance policy, actuate, or
clear history. A bounded transport interruption also blocks that control cycle
without changing accepted estimator state. The next accepted sample must still
satisfy the maximum-gap rule; an over-gap clears reclaim readiness. Missing or
invalid content, stale evidence, a non-increasing-but-different record, or a
retired session clears readiness immediately. A new valid sample may still
raise desired immediately.

Without a qualified history checkpoint, initialize both durable desired and
safe floor conservatively around live allocation: `desired = max(C,
desired_now)` and `safe_floor = C`. This permits immediate growth and prohibits
shrink until the complete warm-up window exists.

These `desired` calculations describe the current single-VM implementation.
The replacement assessment retains the same checked geometry and history
safety but is adapted to total-RAM `demand_target` and `safe_floor`. Once HPM3
is qualified, the host RAM pool ledger is the only component that can turn that
request into `pool_grant` and device-scoped `desired`; this direct assignment is
removed.

The host stores a versioned, bounded, durably flushed and atomically replaced policy checkpoint
containing the VM/alias, policy and compatibility fingerprints, desired,
qualified candidate history, telemetry identity/order, and durable actuation
latch. Missing estimator state restarts cold. Malformed or oversized state
fails closed because it could contain control state; an identity or fingerprint
mismatch also fails closed when a latch or command intent is present. Safely
mismatched estimator-only state restarts reclaim warm-up. A new Windows producer
session also restarts reclaim warm-up.

`virtio-mem-host status` reads that checkpoint and a fresh alias-scoped live
state to emit the versioned status contract. It is observational only: it does
not read a new telemetry sample, update estimator history, resolve an intent,
clear a latch, or send a resize command. A missing checkpoint is reported as a
cold status around current allocation. Malformed or mismatched durable control
state remains an error rather than being summarized as healthy.

## Reconciliation

The reconciler consumes one fresh granted target plus fresh live `Q` and `C`.
Before host-pool integration, the single-VM estimator supplies that target for
bounded qualification. After HPM3, only the durable host-pool grant may supply
it. `current` remains accounting authority; `requested` is device intent;
`desired` is granted policy intent. The control state is separate from all
three values.

The per-VM reconciler does not compare priorities, choose donors, or infer free
pool capacity. A growth command requires an owned pool reservation. A reclaim
command requires a pool plan naming an unmet higher-priority recipient plus the
guest-specific shrink-safe floor. The pool does not request reclaim merely
because a VM is lower priority or currently underutilised.
Released bytes remain unavailable to another VM until authoritative live
`current` confirms them and the pool ledger records the observation.

| Live state | Reconciler action |
| --- | --- |
| `Q == C < desired` | Grow by at most the configured growth quantum toward `desired`, capped at `effective_max`, after host-headroom reservation. |
| `Q == C > desired` | If automatic shrink is enabled and history is ready, reclaim by at most the configured reclaim quantum without crossing `desired`, `safe_floor`, or the configured minimum. |
| `Q > C` | Observe pending growth. Never overlap or supersede it. A timeout changes health, not desired. |
| `Q < C` and `desired <= Q` | Observe pending shrink. Never send a second lower target. |
| `Q < desired < C` | Supersede upward to aligned `desired`, reducing but not reversing the remaining shrink. |
| `Q < C <= desired` | Cancel shrink to `C`; after convergence a later cycle may issue bounded growth. |
| Divergence not matched by durable intent | Enter `recovery_required`; do not claim ownership or replay a command. |

Upward supersession is the only normal exception to the converged-state
precondition. Immediately reread the selected alias and revalidate attestation,
geometry, target, and ownership before applying it. The new target must be
strictly above `Q` and no higher than the freshly observed `C`; growth beyond
`C` occurs only after cancellation converges.

If telemetry becomes stale during an owned shrink, use fresh live device state
to attempt one upward freeze at observed `C`, then latch automatic actuation.
This safety action prevents an already accepted reclaim from continuing while
demand is unknown. If ownership or live state cannot be proven, enter
`recovery_required` without mutation.

Partial shrink progress is useful: every lower `current` immediately becomes
the accounting value. It does not lower desired or release a second allocation
until observed. Zero progress, partial progress, and elapsed deadlines produce
`constrained` health. They do not rewrite desired, issue a lower target, or
enable diagnostic re-notification.

### Command intent and ambiguity

Before any command, atomically record an intent containing operation ID,
direction, prior `Q`/`C`, target, telemetry identity, and compatibility/policy
fingerprints. After a success or error, immediately reread live state:

- live `requested == target`: the command was accepted; observe convergence;
- live state equals the recorded prior state: the command was not applied;
  latch and require a later explicit clear rather than replaying automatically;
- any other result: mark `command_unknown` and latch.

On restart, compare the journal with live state using the same rules. Resume
observation but never replay a recorded command. The durable latch survives
service-manager restart and is cleared only by a documented operator action
after live state is converged or deliberately recovered. The host provides a
bounded `clear-latch` CLI operation that is dry-run by default, rereads live
state, validates VM/alias/fingerprints, refuses divergence, and records the
operator-visible reason for the clear.

An explicit clear may migrate a compatibility fingerprint after the replacement
attestation has itself passed live validation. The checkpoint must still match
the exact VM, alias, and policy fingerprint, and live `requested` must equal
`current`. Applying that reviewed recovery clears command intent, restarts
estimator/reclaim history cold, records the reason, and never replays the old
operation. Policy or identity drift remains a hard refusal.

## Qualification

Qualification has two separately reported outcomes:

1. **Controller qualification:** the estimator and reconciler meet this
   contract under deterministic clocks, generated boundary cases, injected
   I/O faults, command ambiguity, cancellation, and restart.
2. **Platform reclaim qualification:** the selected Windows/QEMU/libvirt stack
   demonstrates repeatable bounded reclaim and safe pressure cancellation
   under the declared workload.

Automatic shrink remains enabled by default regardless of qualification
status, because it is a core product function. An unqualified or failing
deployment remains protected by warm-up, default-off re-notification,
single-request convergence, and durable latching; its health must clearly say
`unqualified`, `constrained`, or `latched` rather than silently behaving as a
growth-only service.

Hermetic tests must prove arithmetic, alignment, effective maximum, base drift,
history gaps, warm-up, immediate growth, delayed reclaim, safe-floor ordering,
bounded quanta, upward-only supersession, stale-during-shrink freeze, partial
progress accounting, command resolution, journal recovery, and no replay.

The bounded live workload uses a Rust helper to distinguish committed-but-
untouched memory from committed-and-touched resident memory. Each run supplies
its allocation sizes, safety cap, phase durations, sampling interval, command
bound, and separate required initial-growth, settled-reclaim, and
renewed-growth deltas. It records zero/partial/full progress,
desired/requested/current, Windows telemetry, controller actions, host
headroom, and workload identity. The analyzer rejects telemetry continuity
failures and an observed lower request before the prior request converges. A
run intended to prove pressure cancellation or supersession additionally
requires renewed pressure to begin while the last observed settled-phase
sample still has `requested < current`.

The maintained qualification workflow is the sole resize authority for its
run. It requires the installed unit to begin disabled and inactive, uses one
bounded privileged guard to start only that unit, verifies automatic shrink is
enabled in the running process, and restores the inactive state on completion,
failure, or timeout. The same elevated child owns the run's fixed libvirt and
production QGA file reads, publishing typed alias-scoped snapshots without
repeated Polkit authorization. Its unprivileged observer validates and
correlates those snapshots with the workload evidence and never writes a
resize target.

When a new Windows telemetry session is required for a live run, qualification
may explicitly cycle only the named telemetry service after controller
ownership begins. It then requires a matching `controller_decision` from that
new session before applying workload pressure. The restart and handoff wait
have their own declared hard bound, are included in the controller ownership
bound, and are recorded as evidence; durable acknowledgement is never edited
or bypassed.

The separate Windows `virtio-mem-workload` helper implements the bounded guest
demand phases and versioned workload evidence. Its native build/tests pass;
that does not qualify the platform until a production-controller run supplies
the correlated host, guest-health, actuation, and recovery evidence above.

Controller qualification may pass when a zero- or partial-progress shrink is
handled correctly. Platform reclaim qualification requires a predeclared set
of repeated representative shrink operations to converge without unsafe
overlap, target undershoot, stale-data actuation, or manual guest restart.

## Host-pool handoff

The HPM0 shared-core contract consumes total-RAM `demand_target` and `safe_floor`,
pressure, and shrink eligibility plus authoritative `requested`/`current`; it
does not reproduce guest-demand calculation. Pool arbitration reserves
capacity atomically only after HPM1; the current pure plan has no dispatch
authority. It grants every eligible request that fits without using priority.
Under contention it may grant total RAM no greater than demand or
reclaim for an unmet higher-priority recipient from a strictly lower-priority
donor toward no lower than `max(minimum, safe_floor)`. Equal- or
higher-priority members are not donors for that request. The host converts the
total-RAM `pool_grant` to an alias-scoped device target using its
non-reclaimable base with checked subtraction and fresh block/device geometry.
Controlled growth or reclaim later passes
that target through the reconciler, which continues to enforce configured
quanta, upward supersession, convergence, journal, and latch rules. Released
capacity is not available to another VM until live `current` confirms it and
the ledger records the observation.
