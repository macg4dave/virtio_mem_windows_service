---
mode: agent
description: Carry out one PLAN.md consolidation register entry (KEEP/IMPROVE/COMBINE/DELETE/SEPARATE)
---

Register entry: `${input:entry:number and title from PLAN.md}`

1. Re-verify the evidence: line numbers may have moved. Search the whole
   workspace for callers, re-exports, CLI dispatch, xtask, Make targets, and
   `.vscode/tasks.json` uses.
2. State the contracts and failure behaviour that must survive, such as
   uncertain outcomes, latches, restart ownership, and live re-read race checks.
3. Choose the canonical implementation. Move callers to it, then delete the
   alternative together with its exports, error variants, and dependencies.
4. Move behavioural tests from the deleted path onto the kept path. Delete
   tests that only checked the shape of the removed implementation, and say why.
5. Search again to confirm nothing still references the removed path. Run the gate.
6. Delete the register entry and any matching execution-order row from
   `PLAN.md`, or cut them down to what remains.

Keep behaviour-preserving refactoring separate from deliberate behaviour
changes. If the entry needs a policy decision, stop and ask.
