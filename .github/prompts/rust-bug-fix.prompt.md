---
mode: agent
description: Fix a Rust bug test-first with a regression test
---

Bug: `${input:bug:observed behaviour, inputs, and expected behaviour}`

1. Reproduce it in a focused unit test next to the owning code. Use fake
   clocks, observations, and injected adapters; never a real VM or libvirt.
2. Run the test and confirm it fails for the reported reason.
3. Fix the cause at its single authority. Do not patch callers, add a special
   case, or weaken an assertion to make the test pass.
4. Check for the same defect in sibling paths (other telemetry source, durable
   versus pending state, restart). Fix them in the canonical path or report them.
5. Run the full Linux gate. If the bug was listed in `PLAN.md`, delete that
   entry once fixed.
