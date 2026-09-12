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
- If the task contains multiple privileged operations or bounded waits, encode
  their complete order in the typed workflow and keep that one elevated child
  alive until the batch succeeds, fails, rolls back, or reaches its declared
  deadline. This is the supported one-password equivalent of `sudo -s`.
- The privileged child must reject non-root direct invocation, perform only the
  reviewed operation, and emit typed output. The unprivileged parent owns
  evidence persistence, cleanup, and result classification.
- Show the complete execution notice before elevation. Never accept arbitrary
  commands, open a root shell, use nested elevation, depend on cached sudo
  credentials, or automate credentials.
- While the batch is running, block on the existing process or documented
  durable completion artifact. Do not repeatedly poll unchanged logs or use
  agent turns merely to report elapsed time; resume on changed output, process
  exit, artifact completion, or the declared timeout boundary.
