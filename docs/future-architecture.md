# Future Architecture

## Current boundary

The implemented system supports one trusted Windows development guest and one
explicit virtio-mem device. Windows publishes native measurements. A Linux
host controller joins them with live device state, calculates an absolute
single-VM target, and performs fail-closed actuation.

The next architecture step is a Windows-native pressure measurement and shadow
assessment layer, not additional guest authority. Global host-capacity
arbitration is the product destination, not optional follow-up work. It consumes
OS-neutral per-VM demand reports rather than freezing the fixed-headroom
estimator or Windows raw schema into the multi-VM design.
See [windows-native-pressure-controller.md](windows-native-pressure-controller.md).

## Global pool

One configured host-wide VM RAM pool declares total guest-RAM pool bytes and,
per member, total-RAM minimum/maximum, priority, exact VM/device identity, and
demand-provider kind. Each VM's charge includes host-derived non-reclaimable
base RAM plus live virtio-mem `current` and outstanding growth reservations;
only the device portion is actuated.
The pool manager consumes, per VM:

- authoritative current allocation;
- provider-assessed `demand_target`, shrink eligibility, and safe floor;
- demand freshness and pressure state;
- configured priority;
- controller health and command ownership.

It reserves every enabled member's minimum before assigning discretionary
capacity. Host available capacity and reserve remain independent safety gates;
they can constrain but never expand the configured pool. The manager must
atomically reserve capacity before granting growth, account reclaimed capacity
only after observing lower current allocation, and reject stale demand without
inventing new targets.

Priority is unused while all eligible growth fits, so lower-priority VMs may
consume available pool RAM up to demand. When growth requests contend for
insufficient capacity, priority orders the remaining grants. If a
higher-priority request remains unmet, only a strictly lower-priority,
underutilised, shrink-safe VM may donate, never below
`max(minimum, safe_floor)`. No waiting higher-priority recipient means no
priority reclaim; an unsafe donor means the recipient waits.

The single-VM reconciler remains responsible for device alignment,
desired/requested/current transitions, command journaling, partial progress,
uncertain results, cancellation, and durable latching.

Windows and future Linux providers retain OS-native telemetry and validation.
Host adapters translate their evidence to one versioned `GuestDemandReport`;
the pool planner contains no Windows- or Linux-specific branches.

## Delivery order

The normative order is the WN0-WN10 guest-provider track followed by the
dependent HPM0-HPM6 system track in [roadmap.md](roadmap.md). Pool construction
may proceed hermetically, but live multi-VM integration cannot precede stable
per-VM assessment, provider qualification, and durable atomic reservation.
WN10 is a component checkpoint; HPM6 owns the final release decision. This
document does not maintain a second delivery sequence.

## Security and support

The current Windows virtio-mem integration is technology preview for a trusted
development guest. Future untrusted or production use requires a separate
threat review, hard hypervisor/cgroup memory limits, least-privilege deployment,
artifact provenance, and supported upstream behavior.

A user-mode driver status interface is not required for allocation accounting.
If a future diagnostic need justifies one, it must be versioned, read-only,
access-controlled, externally built and signed, tested on a disposable target,
and kept advisory.

## Non-goals

- Windows-side libvirt or resize commands;
- allocation inferred from Windows totals, QGA, or balloon counters;
- independent controller instances racing for a shared host pool;
- direct per-VM allocation or reclaim remaining as a compatibility mode after
  pool-owned replacements qualify;
- replaying commands after restart;
- embedding release-specific VM names, sizes, durations, or paths in design
  documentation.
