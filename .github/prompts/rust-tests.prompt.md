---
mode: agent
description: Add focused Rust tests for memory math, parsing, telemetry, and control state
---

Area under test: `${input:area:module or behaviour}`

- Test observable behaviour through the module's real entry point, not
  private constants or implementation shape.
- Cover boundaries that matter here: block alignment, device `[min, max]`,
  `u64` overflow, KiB/byte conversion, fixed base RAM versus device bytes,
  stale/future/replayed/altered telemetry, and `requested` versus `current`.
- For control state, cover convergence, rejection before execution, unknown
  outcome, timeout, latch persistence, invalid checkpoints, and restart.
- Use fake clocks and injected adapters, temporary directories for
  persistence, and the existing local subprocess fixture pattern
  (`host/src/virsh.rs` tests) for process behaviour.
- `expect` with a message is fine in tests; keep each test's name a sentence
  describing the behaviour.
- Run the new tests and the Linux gate. Report any test that exposes a real
  defect rather than adjusting it to pass.
