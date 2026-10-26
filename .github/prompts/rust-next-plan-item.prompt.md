---
mode: agent
description: Implement the next open PLAN.md item in Rust, then delete it from PLAN.md
---

Implement one open item from `PLAN.md`: `${input:item:register entry, task row, or phase step}`.
If no item is given, take the lowest-numbered open row in "Refactoring
execution order" that has no unmet prerequisite.

1. Quote the item's acceptance criteria before writing code. If they are
   ambiguous, or the item needs live VM work, stop and ask.
2. Trace the entry points, callers, and tests the item touches. Name the one
   canonical implementation that will remain.
3. Make the smallest change that meets the criteria. Remove superseded paths,
   exports, settings, and dependencies in the same change.
4. Add or move focused tests that prove the criteria; a fix needs a regression
   test seen failing first.
5. Run focused tests, then
   `cargo test --workspace --exclude virtio-mem-service --locked`,
   `cargo fmt --all -- --check`, and
   `cargo clippy --workspace --exclude virtio-mem-service --locked -- -D warnings`.
6. If everything passes, **delete** the finished item from `PLAN.md` and trim
   any text it made stale. Keep facts that still constrain open work. If only
   part is done, cut the item down to what remains.
7. Report what changed, what ran, and what did not run.
