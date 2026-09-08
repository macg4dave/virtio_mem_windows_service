---
name: rhel-privileged-batch
description: Batch a task-scoped RHEL live-validation operation behind one sudo prompt
---

Read `.github/copilot-instructions.md`, `docs/testing.md`, and `BACKLOG.md`
before preparing the batch.

Task:
"""
<the operator's requested RHEL virsh/systemctl task>
"""

Use this workflow when a single beta build, test, service-lifecycle, resize, or
inspection task needs privileged `virsh`, `systemctl`, `journalctl`, or closely
related host commands. Task-scoped live validation is authorized by default;
do not pause for a separate approval when the target and safe bounds are clear.

1. Perform safe unprivileged discovery first. Resolve every VM name, device
   alias, systemd unit, artifact, output path, and requested mutation before
   execution. Stop for clarification if the target is ambiguous.
2. Create one task-specific Bash script under the Git-ignored directory
   `.vscode-artifacts/privileged-tasks/`. Use a descriptive filename, an
   absolute workspace path when invoking it, `#!/bin/bash`, and
   `set -euo pipefail`.
3. Put the entire task-scoped privileged sequence in that script. Use fixed
   command paths and quoted argument arrays where practical. Commands inside
   the script must never invoke `sudo`, `su`, or `doas`.
4. Do not accept commands, shell fragments, VM names, unit names, or output
   paths from an untrusted file, standard input, or an open-ended loop. Embed
   the resolved task scope in the script so the operator can review exactly
   what will run.
5. For service or resize mutations, include precondition checks, bounded
   waits/timeouts, convergence checks, and a concrete rollback to captured
   initial state. Do not bypass repository safety gates or combine unrelated
   mutations merely to reduce password prompts.
6. Before execution, show the operator the script path and summarize every
   target, mutation, expected effect, timeout, output file, and rollback. This
   is an execution notice, not an approval request. Then invoke exactly:

   ```bash
   sudo bash /absolute/workspace/.vscode-artifacts/privileged-tasks/TASK.sh
   ```

7. Invoke that command once in an interactive terminal. The operator types the
   sudo password directly if prompted; never request, read, echo,
   transmit, cache, or automate it.
8. If authorization or any command fails, stop and report the failure. Do not
   split the batch into individual sudo calls, retry it piecemeal, or depend on
   sudo timestamp caching.
9. Report the commands' results, final live state, and whether rollback ran.
   The script is valid only for the current task; do not reuse it for unrelated
   work.

For output that should be owned by the regular user, prefer writing command
output to stdout and redirecting the single outer invocation from the user's
shell. Never let a privileged script follow an unverified user-controlled
symlink.
