---
mode: agent
description: Remove an unused Rust implementation and everything that only it needed
---

Target: `${input:target:type, function, module, or feature}`

1. Prove it is unused in production: search for callers, re-exports, trait
   impls, CLI dispatch, xtask, and the Windows crate. Treat public library
   exports as possible external contracts; report them, don't assume either way.
2. If it is CLI-reachable or part of a wire format, this is feature
   retirement, not dead-code removal: stop and confirm with the user.
3. Delete it with its exports, tests that only tested it, error variants,
   config settings, and Cargo dependencies that become unused.
4. Run `cargo build`, the Linux gate, and Clippy; confirm no warnings about
   newly unused items remain.
5. Delete the matching `PLAN.md` entry, or cut it down to what remains.
