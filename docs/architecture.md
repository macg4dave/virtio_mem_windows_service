# Architecture

## Supported scope

The current system coordinates one explicitly configured trusted Windows guest
and one virtio-mem device from one RHEL host controller. Multi-VM arbitration,
untrusted guests, and production support require the host RAM pool architecture
described below.

The destination is one host-wide VM RAM manager with exclusive allocation
authority over a configured RAM pool. The pool configuration declares the
total bytes available to managed VMs and, for each member, its exact identity,
minimum guarantee, maximum bound, priority, virtio-mem device, and guest-demand
provider. Per-guest controllers remain useful components, but they do not own
capacity independently and are not the top-level product architecture.

Priority is a contention rule, not a standing reservation or target share.
When the pool has enough unreserved capacity for every eligible growth request,
all VMs may grow toward demand regardless of priority. Priority becomes active
only when requests compete for insufficient capacity or a higher-priority VM
has unmet demand while lower-priority VMs hold safely reclaimable memory.

Pool capacity and per-VM minimum/maximum values mean total guest RAM, not only
the hotpluggable virtio-mem portion. Each member's pool charge includes its
host-derived non-reclaimable base memory plus authoritative live virtio-mem
`current` and any durable growth reservation. The manager converts a total-RAM
`pool_grant` to the alias-scoped device target (`desired`) for actuation. Guest
counters never supply allocation accounting.

## Ownership

### Guest telemetry providers

The Windows Rust service owns native memory measurement and pressure-signal
normalisation, versioned atomic raw telemetry publication, local configuration
and ACLs, SCM lifecycle, and Windows observability. It never receives host
allocation, calculates a host resize, or invokes QGA/libvirt/Linux commands.
The installed QEMU Guest Agent owns the QGA virtio-serial channel.

Other guest operating systems may use their own native collectors and
transports. Raw Windows and Linux records do not need a common wire schema.
Each host-side provider adapter validates its OS-specific evidence and produces
the same versioned per-VM demand assessment: quantitative demand, pressure,
shrink eligibility and safe floor, freshness, identity, and reason codes. A
provider never sees the shared pool or chooses an allocation.

### Per-VM assessment and reconciliation

For each VM, host Rust owns raw-telemetry validation and replay protection,
alias-scoped live XML/QMP state, compatibility attestation, demand assessment,
and the safe floor below which the guest must not be reclaimed. The assessment
is a request for pool capacity, not an allocation decision.

After arbitration, the per-VM reconciler owns
`desired`/`requested`/`current` reconciliation, intent journaling, actuation,
convergence, latching, and recovery for its exact VM/device. It may pursue only
the target granted by the host pool manager.

Live libvirt `current` is allocation authority. `requested` is device intent;
guest counters are demand evidence only. A lower demand assessment does not
release bytes until the corresponding live `current` has actually fallen.

The implemented estimator currently converts available physical memory and
commit headroom into fixed-reserve candidates. The release direction replaces
that primary sizing input with a Windows-native pressure assessment while
preserving host ownership. See
[windows-native-pressure-controller.md](windows-native-pressure-controller.md).

### Host RAM pool manager

One host-wide coordinator owns the configured VM pool, membership, priority
policy, durable reservations, arbitration, and dispatch of per-VM grants. Each
cycle uses one coherent snapshot containing every member's minimum, priority,
fresh demand assessment, safe floor, `requested`, and authoritative `current`.

The manager protects every enabled member's minimum before distributing
discretionary capacity. The sum of aligned minimum guarantees must fit within
the configured pool. Capacity above those guarantees is shared by current
demand; priority does not reserve or withhold free capacity while the pool is
unconstrained.

When eligible growth exceeds free capacity, the manager first assigns the
remaining unreserved bytes among the contending requests according to the
configured priority and deterministic same-priority rule. If a
higher-priority VM still has unmet demand, the manager may then plan reclaim
only from strictly lower-priority VMs that are underutilised and whose provider
reports shrink-safe evidence. A donor never crosses the greater of its minimum
and safe floor. Equal- or higher-priority VMs are not shrunk to satisfy a
request, and underutilisation alone does not trigger reclaim. If safe
lower-priority reclaim cannot satisfy the request, the recipient waits in an
explicit capacity-constrained state.

Accepted growth is reserved durably before dispatch so concurrent reconcilers
cannot spend the same bytes. Planned reclaim is not reusable capacity until
live `current` confirms release; the higher-priority recipient remains waiting
until then. Stale membership, incomplete snapshots, ambiguous ownership,
ledger corruption, or arithmetic failure blocks new allocation and reclaim
rather than falling back to independent per-VM action.

Non-reclaimable base memory always remains charged to its VM. Reclaimability is
therefore limited both by the guest's total-RAM minimum/safe floor and by the
device's own lower bound and geometry.

### Shared core

The shared crate owns checked byte units, virtio-mem XML parsing and geometry,
memory policy, target estimation, reconciliation, recovery state machines, and
versioned evidence types. HPM0 adds the version-1 semantic `HostPoolPolicy`,
OS-neutral `GuestDemandReport`, and deterministic `HostPoolPlan` contracts. Its
pure planner accepts an explicit total-guest-RAM pool, charges host-derived
non-reclaimable base plus live device allocation, grants unconstrained growth
without priority, and emits only named reclaim-for-transfer dependencies. It
does not reserve durably, coordinate processes, or actuate. Pure logic remains
independent of live systems.

HPM1 adds the version-1 durable `PoolLedger` beside that planner. The ledger is
bound to a canonical complete pool-policy fingerprint, accounts observed
total-RAM allocation plus growth reservations exactly once, retains pending
reclaim as charged until `current` falls, and gives each staged resize one
unique plan-generation owner. Its bounded checksum-protected file is flushed
and atomically replaced. Restart recovery classifies fresh live state without
replaying commands; incomplete, drifted, corrupt, over-capacity, or ambiguous
state fails closed. This is a shared state machine and persistence primitive,
not the HPM2 coordinator or an actuation path.

### Build and validation control plane

`cargo xtask` owns aggregate repository gates, remote native-Windows
orchestration, artifact verification, environment checks, QGA readiness,
reversible live-resize orchestration, and unattended qualification. It does not
replace focused Cargo tests or become a second product controller.

Editor and Make entrypoints delegate to `xtask`. Privileged workflows validate
their scope before one `sudo` re-execution of the current prebuilt xtask; Cargo
and evidence persistence remain unprivileged.

## Data flow

```text
Windows telemetry     Linux telemetry     future guest telemetry
        |                    |                       |
        +------ OS-specific host provider adapters -+
                             |
                 per-VM demand/pressure assessment
                             |
          + authoritative requested/current allocation
                             |
          host RAM pool manager and durable reservation ledger
                             |
              minimums + priorities + available pool
                             |
                    per-VM total-RAM pool grant
                             |
                 per-VM device target (`desired`)
                             |
          per-VM desired/requested/current reconciliation
                             |
            journaled, alias-scoped virtio-mem actuation
```

The host acknowledges accepted telemetry durably. Missing, stale, malformed,
replayed, cross-VM, or incomplete records cannot authorize reclaim.

For the production single-VM handoff, the host uses standard bounded QGA
guest-file operations to read the explicitly configured protected Windows
current record. It does not use guest execution, SSH copying, a guest-writable
host share, or a custom memory command. The QGA handle is closed after every
attempt, and accepted-session acknowledgement remains host-owned durable state.

## Configuration

The destination configuration model has one host-wide pool document. Its
semantic fields, independent of the eventual file syntax, include:

- a total VM-pool byte limit that never grows implicitly from host free memory;
- an explicit set of managed VMs;
- per-VM identity and device alias, minimum guarantee, maximum bound, priority,
  demand-provider kind and provider-specific settings; and
- host safety reserves, freshness/timing bounds, durable state paths, and
  arbitration policy/version.

The configured VM-pool limit is the allocation budget. Fresh host physical
headroom remains an independent safety gate and may further constrain growth;
it never silently expands the pool. QEMU/process overhead and other host
consumers are protected by those host reserves; they are not mislabeled as VM
RAM grants. Non-reclaimable base memory comes from reviewed host topology.
Device size, block geometry, `requested`, and `current` are derived from fresh
alias-scoped live state. Configuration is invalid when aligned total-RAM
minimum guarantees or an existing allocation cannot be represented safely
within the declared pool. Membership and the treatment of an inactive VM are
explicit, so a guaranteed minimum is not silently lent out.

Priority configuration defines contention ordering only. It does not create a
per-VM quota, pre-reserve capacity above the minimum, or prevent a
lower-priority VM from using otherwise free pool RAM.

During the incremental migration, existing per-instance configuration remains
only where the current single-VM implementation requires it. New policy is
defined against the host-wide model and does not gain permanent compatibility
aliases.

During the pressure-aware migration, signal availability and degradation are
explicit configuration/telemetry states. Missing required pressure evidence
blocks shrink; it is never interpreted as a zero counter.

The Windows versioned configuration file is required. The host instance file
is validated before service startup. Configuration changes that affect
compatibility require a fresh reviewed attestation.

## Actuation safety

- Only the host issues resize requests.
- Only the host-pool manager grants capacity; a per-VM assessor or reconciler
  cannot allocate outside its current durable grant.
- A VM minimum and qualified safe floor are hard lower bounds; priority applies
  only to capacity that can safely move above those bounds.
- Priority cannot trigger reclaim while all eligible demand fits. A transfer
  requires unmet higher-priority demand and a strictly lower-priority,
  shrink-safe donor.
- A growth reservation is counted before command dispatch, while reclaimed
  capacity is counted only after authoritative `current` decreases.
- Targets are checked byte counts aligned to the current device block.
- An ordinary request is prohibited while requested and current differ.
- Every request uses fresh compatibility, headroom, telemetry, and live-state
  evidence and is journaled before command execution.
- Ambiguous command outcome is resolved by live reread; unknowable or stalled
  state latches durably rather than retrying blindly.
- Reclaim capability remains default-on with a deliberate pause override. In
  the destination it is scheduled only by the pool manager for unmet
  higher-priority demand, not proactively by a per-VM loop.
- Normative target, quantum, history, hysteresis, notification, and recovery
  constants are defined only in `target-controller.md` and owning Rust code.

## Incremental convergence and removal

The current single-VM controller is retained as a qualification scaffold for
guest-demand assessment and per-VM reconciliation. It is not a second long-term
allocation architecture. Milestones first make its demand output explicit,
then insert the host-wide pool in shadow mode, then require a durable pool grant
for growth, and finally move reclaim arbitration to the pool manager.

Once each replacement is qualified, the superseded path is removed in the same
or immediately following cleanup slice. In particular, the fixed-headroom
estimator, threshold demand mode, direct per-VM capacity choice, and independent
per-VM reclaim are not permanent compatibility modes. A temporary decoder or
fallback may exist only for a named deployment/rollback window with an owner,
exit gate, and deletion task; release documentation must not promise indefinite
support for it.

## Lifecycle and deployment

The Windows service uses wakeable cancellation and configured operation,
polling, and shutdown bounds. Unexpected worker failure remains non-zero and
observable.

The checked-in host unit is fail-stop. Deployment monitoring decides whether
and when a reviewed restart is appropriate; repository defaults do not encode
a retry loop learned from testing.

Candidate build/test and hash verification run through `cargo xtask`. Product
service installation uses the product CLI under the target platform's required
privilege. RHEL elevation uses one reviewed typed xtask re-execution; no shell
script or root Cargo process is part of the workflow.

## Validation

Focused Rust tests prove deterministic behavior. Local, native Windows,
deployment, and live workflows remain separate result layers. Live tests use
explicit targets and run-specific bounds, capture initial and final state, and
define cleanup/rollback before mutation. See `testing.md`.
