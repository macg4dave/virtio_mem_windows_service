---
name: rhel-privileged-batch
description: Create one exact reviewable RHEL elevation boundary
---

Read `docs/testing.md` and the selected workflow contract.

Task: `<exact privileged operation already required by the task>`

- Resolve every target, input, command, output, effect, time bound, and rollback
  through unprivileged discovery first.
- Create one task-specific script under
  `.vscode-artifacts/privileged-tasks/` with `#!/bin/bash` and
  `set -euo pipefail`.
- Embed fixed reviewed arguments for this task. Do not accept arbitrary shell
  input, use nested elevation, or duplicate Rust parsing/policy/polling.
- Invoke prebuilt `cargo xtask` or product Rust behavior for reusable work.
- Show the complete execution notice, then invoke the script once with one
  outer `sudo`. Never automate credentials.
- Delete the script when its evidence-retention need ends; it is not an API.
