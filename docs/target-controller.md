# Quantitative Target Controller Contract

## Status and scope

This is the normative contract for absolute per-VM target estimation,
asynchronous virtio-mem reconciliation, and qualification. It replaces
directional demand estimates with an explicit desired target.

Automatic Windows shrink is a default-on product capability. A deployment may
set `VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK=false` for diagnosis or a deliberate
rollout pause, but absence of that setting means enabled. Enabled reclaim still
requires fresh continuous telemetry, a warmed history window, a valid safe
floor, compatible device geometry, current compatibility attestation, and an
unlatched reconciler. Same-target re-notification remains a separate default-
off diagnostic feature and is not part of normal target reconciliation.

This contract remains single-VM until the global controller supplies atomic
reservation.
Windows publishes measurements only; the RHEL controller owns calculation,
host safety, reconciliation, and actuation.

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
- `desired`: the stable absolute policy target produced by the estimator.
- `safe_floor`: the lowest target that pressure arbitration may consider from
  the current qualified history. It is not an actuation command.

`memory_load_percent`, `commit_peak_bytes`, cache, and kernel pool values remain
diagnostic and consistency evidence. `commit_peak_bytes` is a since-boot peak
and must not be used as rolling demand. Commit charge is a virtual-memory
constraint rather than a direct measure of resident physical use, so physical
and commit candidates are calculated separately and combined with `max`, never
added together.

## Target estimator

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

The fixed base and state path are required. A derived maximum-gap value is
materialized and validated at startup so a later poll-interval change cannot
silently alter an active history policy.

All reserves and hysteresis must be positive, block-alignable, ordered so floor
reserves do not exceed normal reserves, and configurable in canonical bytes.

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

The host stores a versioned, bounded, durably flushed and atomically replaced policy checkpoint
containing the VM/alias, policy and compatibility fingerprints, desired,
qualified candidate history, telemetry identity/order, and durable actuation
latch. Missing estimator state restarts cold. Malformed or oversized state
fails closed because it could contain control state; an identity or fingerprint
mismatch also fails closed when a latch or command intent is present. Safely
mismatched estimator-only state restarts reclaim warm-up. A new Windows producer
session also restarts reclaim warm-up.

## Reconciliation

The reconciler consumes one fresh estimator result plus fresh live `Q` and `C`. `current`
remains accounting authority; `requested` is device intent; `desired` is policy
intent. The control state is separate from all three values.

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
bound, and required observed deltas. It records zero/partial/full progress,
desired/requested/current, Windows telemetry, controller actions, host
headroom, and workload identity.

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

## Global-controller handoff

The global controller consumes `desired` and `safe_floor` plus authoritative `current`; it
does not reproduce guest-demand calculation. Global arbitration reserves host
capacity atomically and may grant a target no greater than desired or reclaim
toward safe floor. Controlled reclaim passes that grant through the reconciler,
which continues to enforce the configured quanta, upward supersession,
convergence, journal, and latch rules.
