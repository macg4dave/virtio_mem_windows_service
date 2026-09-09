---
name: rhel-privileged-batch
description: Batch a task-scoped RHEL live-validation operation behind one sudo prompt
---

Read `.github/copilot-instructions.md`,
`.github/prompts/workflow-validation.prompt.md`, `docs/testing.md`, and
`BACKLOG.md` before preparing the batch.

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
   the script must never invoke `sudo`, `su`, or `doas`. Invoke the already
   built `target/release/virtio-mem-xtask` or product Rust CLI for reusable
   parsing, validation, polling, and resize logic; do not copy it into Bash.
4. Do not accept commands, shell fragments, VM names, unit names, or output
   paths from an untrusted file, standard input, or an open-ended loop. Embed
   the resolved task scope in the script so the operator can review exactly
   what will run.
5. For service or resize mutations, include precondition checks, bounded
   waits/timeouts, convergence checks, and a concrete rollback to captured
   initial state. Do not bypass repository safety gates or combine unrelated
   mutations merely to reduce password prompts.
6. For a VM or guest lifecycle task, add a layered guest-health gate; QGA
   availability alone is not proof that Windows is usable:
   - Before mutation, capture the domain UUID/ID and the QEMU process identity
     and start time, then require `domstate` running, QGA ping, an independent
     authenticated Windows command, and the named Windows services or
     application endpoint needed by the test.
   - Inspect pending-reboot indicators and active installer/update activity.
     Stop before mutation if an unrelated reboot can overlap the bounded test.
   - Capture a Windows boot marker and a bounded System event-log baseline.
     For a guest-only reboot, require the QEMU process and domain identity to
     remain continuous, the boot marker to change exactly once, and the
     expected planned-reboot event to identify the requested initiator.
   - Treat Kernel-Power 41, unexpected-shutdown 6008, BugCheck 1001, an
     unexpected 1074 initiator, a second boot transition, or loss of the QEMU
     process as a failed run. Do not continue into resize, attestation, install,
     or other persistent steps after such a signal.
   - After recovery, require at least three successful end-to-end probes across
     an explicitly bounded quiet window. For a lifecycle test that precedes a
     persistent security or attestation change, default that window to ten
     minutes unless the task documents why a shorter interval covers every
     delayed reboot/recovery mechanism in scope.
7. Build required Rust tooling as the regular user with
   `cargo xtask gate build`. Before execution, show the operator the script
   path and summarize every
   target, mutation, expected effect, timeout, output file, and rollback. This
   is an execution notice, not an approval request. Then invoke exactly:

   ```bash
   sudo bash /absolute/workspace/.vscode-artifacts/privileged-tasks/TASK.sh
   ```

8. Invoke that command once in an interactive terminal. The operator types the
   sudo password directly if prompted; never request, read, echo,
   transmit, cache, or automate it.
9. If authorization or any command fails, stop and report the failure. Do not
   split the batch into individual sudo calls, retry it piecemeal, or depend on
   sudo timestamp caching.
10. Report the commands' results, final live state, and whether rollback ran.
   The script is valid only for the current task; do not reuse it for unrelated
   work.
11. After the task, move repeated prerequisites, evidence capture, polling,
    cleanup, or rollback behavior into `tools/xtask` under a BT task. Delete
    the task script when its evidence-retention need ends; it has no
    compatibility contract.

For output that should be owned by the regular user, prefer writing command
output to stdout and redirecting the single outer invocation from the user's
shell. Never let a privileged script follow an unverified user-controlled
symlink.
