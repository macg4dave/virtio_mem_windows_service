---
mode: agent
description: Review a Rust diff against this repository's rules
---

Review `${input:target:diff, branch, or files}`. Do not edit code.

Check, in priority order:

1. Correctness of memory quantities: `u64` bytes, checked arithmetic,
   alignment, device bounds, base RAM versus device allocation.
2. Control safety: one command in flight, `current` observed rather than
   assumed, no retry of an uncertain resize, latches never silently cleared,
   live state re-read before an effect.
3. Errors: no new `unwrap`/`expect`/`panic!` outside tests, no discarded
   `Result`, causes preserved, rejection distinguished from unknown outcome.
4. Ownership: one authority per responsibility, superseded paths removed,
   no speculative traits, modes, settings, or dependencies.
5. Tests prove the behaviour change; none weakened to pass.
6. `PLAN.md` drops finished items and does not gain progress logs.

Report each finding with file:line, trigger, consequence, and the fix.
Separate verified facts from assumptions.
