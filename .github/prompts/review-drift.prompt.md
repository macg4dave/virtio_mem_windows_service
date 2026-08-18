---
agent: agent
description: Review a virtio-mem Windows service diff for contract and documentation drift
---

Review the current diff against:

- `.github/copilot-instructions.md`
- `BACKLOG.md`
- `docs/engineering-standards.md`
- `docs/architecture.md`
- `docs/feature-matrix.md`
- `docs/api-contract.md` if QEMU Guest Agent or public Rust API behavior changed
- `docs/data-model.md` if persistence changed
- `docs/testing.md`

Lead with concrete findings only. For each finding, name:

- the file and line or section
- the contract that drifted
- the likely runtime or maintenance impact
- the smallest correction

Do not approve the change just because tests pass. Tests are evidence, not a substitute for contract review.

Also check that:

- only Rust and Bash were added or changed;
- Windows service code does not invoke Linux commands or access host devices;
- parsing validates malformed, missing, inconsistent, and overflowing values;
- new public Rust items have appropriate documentation and tests;
- `BACKLOG.md` and affected docs reflect the change;
- no secrets, credentials, tokens, private keys, or production data were introduced.

Shell safety:

- Review commands must be read-only by default. Do not edit/delete server files or mutate VM, libvirt, or systemd state without explicit current-turn approval naming the exact target and action.
- Never use `sudo`, `su`, or `doas` during review without current-turn approval naming the complete command, target, mutation, and rollback. After approval run the whole script once under `sudo`; never automate password entry.
