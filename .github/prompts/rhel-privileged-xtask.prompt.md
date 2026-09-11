---
name: rhel-privileged-xtask
description: Execute one exact reviewable RHEL xtask elevation boundary
---

Read `docs/testing.md` and the selected workflow contract.

Task: `<exact privileged operation already required by the task>`

- Resolve and validate every target, input, command, output, effect, time bound,
  and rollback in the unprivileged `cargo xtask` process first.
- Add or select one capability-named Rust workflow. Do not create or generate a
  shell script and do not run Cargo as root.
- For the privileged phase, have xtask re-execute its current prebuilt
  executable through one outer `sudo` with a fixed argument vector.
- The privileged child must reject non-root direct invocation, perform only the
  reviewed operation, and emit typed output. The unprivileged parent owns
  evidence persistence, cleanup, and result classification.
- Show the complete execution notice before elevation. Never accept arbitrary
  commands, use nested elevation, or automate credentials.
