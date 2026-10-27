---
mode: agent
description: Prepare a bounded manual test of the host controller run with sudo
---

Goal: `${input:goal:what the test should demonstrate}`

Do not run anything against the VM until the user approves the exact plan.

1. Build the host binary: `cargo build -p virtio-mem-host --locked`.
2. Write the plan: VM name, device alias, baseline `requested`/`current` in
   bytes, block size, target in bytes and KiB, host reserve, timeouts, and the
   recovery steps if unplug stalls. The daemon sizes RAM in both directions;
   there is no grow-only mode.
3. Write a test config file from `host/systemd/virtio-mem-host.toml.example`
   (set `observe_only = true` for a run that must not resize). Run it under
   the fake systemd runner, never as an installed unit:
   `cargo xtask host-run --config FILE --max-run-seconds N --min-host-available-bytes BYTES --sudo`.
   It stops the daemon at the run limit, on low host memory, or on Ctrl-C,
   and logs to `.artifacts/host-run/`.
4. The program does all polling and resizing. Do not run a manual virsh loop.
5. Once it has run, record the results (convergence time, stalls, final live
   state) in `PLAN.md`. If this finishes an acceptance step, delete that step.
