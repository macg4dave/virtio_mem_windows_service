---
mode: agent
description: Prepare a bounded manual test of the host controller run with sudo
---

Goal: `${input:goal:what the test should demonstrate}`

Do not run anything against the VM until the user approves the exact plan.

1. Build the host binary: `cargo build -p virtio-mem-host --locked`.
2. Write the plan: VM name, device alias, baseline `requested`/`current` in
   bytes, block size, target in bytes and KiB, host reserve, timeouts, and the
   recovery steps if unplug stalls. Keep shrink disabled unless shrink is
   what is being tested.
3. Write a test config file from `host/systemd/virtio-mem-host.toml.example`
   (set `observe_only = true` for a run that must not resize). Give the exact
   command to run, for example
   `sudo ./target/debug/virtio-mem-host --config FILE`, or
   `sudo ./target/debug/virtio-mem-host status --config FILE`. Running as root
   avoids the repeated polkit password prompts; it is a test path, not the
   deployment.
4. The program does all polling and resizing. Do not run a manual virsh loop.
5. Once it has run, record the results (convergence time, stalls, final live
   state) in `PLAN.md`. If this finishes an acceptance step, delete that step.
