---
mode: agent
description: Add focused behavioural tests for Rust contracts and control state
---

Area: `${input:area:module or behaviour}`

Choose applicable boundaries: alignment, device bounds, overflow, byte/KiB
conversion, fixed base versus device memory, stale/future/replayed/altered
telemetry, and requested versus current allocation. For lifecycle changes,
cover convergence, rejection, uncertain outcomes, timeout, latch persistence,
invalid checkpoints and restart.

Reuse existing fixtures, including `host/src/virsh.rs` subprocess tests, rather
than adding a parallel harness. Report real defects exposed by new tests.
