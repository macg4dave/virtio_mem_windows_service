---
name: rhel-privileged-batch
description: Batch an approved RHEL virsh or systemctl task behind one sudo prompt
---

Read `.github/copilot-instructions.md`, `docs/testing.md`, and `BACKLOG.md`
before preparing the batch.

Task:
"""
<the operator's requested RHEL virsh/systemctl task>
"""

Use this workflow when a single task needs several privileged `virsh`,
`systemctl`, `journalctl`, or closely related read-only host-inspection
commands and the operator wants to authenticate only once.

1. Perform safe unprivileged discovery first. Resolve every VM name, systemd
   unit, output path, and requested mutation before asking for approval.
2. Create one task-specific Bash script under the Git-ignored directory
   `.vscode-artifacts/privileged-tasks/`. Use a descriptive filename, an
   absolute workspace path when invoking it, `#!/bin/bash`, and
   `set -euo pipefail`.
3. Put the entire approved privileged sequence in that script. Use fixed
   command paths and quoted argument arrays where practical. Commands inside
   the script must never invoke `sudo`, `su`, or `doas`.
4. Do not accept commands, shell fragments, VM names, unit names, or output
   paths from an untrusted file, standard input, or an open-ended loop. Embed
   the resolved task scope in the script so the operator can review exactly
   what will run.
5. For mutations, include precondition checks, bounded waits/timeouts, and a
   concrete rollback path where rollback is safe. Do not combine unrelated
   mutations merely to reduce password prompts.
6. Before execution, show the operator the script path and summarize every
   protected target, mutation, expected effect, output file, and rollback.
   Request approval for the exact command:

   ```bash
   sudo bash /absolute/workspace/.vscode-artifacts/privileged-tasks/TASK.sh
   ```

7. After approval, invoke that command once in an interactive terminal. The
   operator types the sudo password directly; never request, read, echo,
   transmit, cache, or automate it.
8. If authorization or any command fails, stop and report the failure. Do not
   split the batch into individual sudo calls, retry it piecemeal, or depend on
   sudo timestamp caching.
9. Report the commands' results and whether rollback ran. The script is valid
   only for the approved task; do not reuse it as authorization for later
   work.

For output that should be owned by the regular user, prefer writing command
output to stdout and redirecting the single outer invocation from the user's
shell. Never let a privileged script follow an unverified user-controlled
symlink.
