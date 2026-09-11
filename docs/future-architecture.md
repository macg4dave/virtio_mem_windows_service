# Future Architecture

## Current boundary

The implemented system supports one trusted Windows development guest and one
explicit virtio-mem device. Windows publishes native measurements. A Linux
controller joins them with live device state, calculates an absolute target,
and performs fail-closed actuation.

The next architecture step is global host-capacity arbitration, not additional
guest authority or another live-test implementation.

## Global pool

The global controller will consume, per VM:

- authoritative current allocation;
- desired target and safe floor;
- demand freshness and pressure state;
- allocation and reclaim priority;
- controller health and command ownership.

It will also consume host available capacity and configured reserve. It must
atomically reserve capacity before granting growth, account reclaimed capacity
only after observing lower current allocation, and revoke stale demand without
inventing new targets.

The single-VM reconciler remains responsible for device alignment,
desired/requested/current transitions, command journaling, partial progress,
uncertain results, cancellation, and durable latching.

## Delivery order

1. Build deterministic multi-VM pool accounting and invariants.
2. Exercise competing growth, reclaim, stale reports, host pressure, restart,
   and uncertain command outcomes.
3. Add controlled target grants through the existing reconciler.
4. Qualify one coherent single-VM deployment before enabling live multi-target
   actuation.
5. Add operational metrics, fault injection, deployment, rollback, and
   explicitly configured endurance evidence.

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
- replaying commands after restart;
- embedding release-specific VM names, sizes, durations, or paths in design
  documentation.
