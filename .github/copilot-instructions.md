# Repository instructions

Read `PLAN.md` first. It is the only project doc: what the project is for,
its honest current state, and the phased plan. Work on the current phase only.

## Code

- Rust only. Keep it small: no new module, trait, mode, or config knob unless
  current code needs it now. Prefer deleting code to adding it.
- The Windows service only measures and publishes memory stats. The host
  daemon decides target size and runs `virsh`. Windows never resizes.
- Memory sizes are `u64` bytes with checked arithmetic, aligned to the device
  block size.
- Add focused unit tests for logic you change (target math, parsing, state).
  Don't add tests that only restate the implementation.

## Validation

- `cargo test -p virtio-mem-core -p virtio-mem-host` (plus `-p virtio-mem-xtask`
  while it exists) must pass. The `windows` crate only builds for a Windows target.
- "Works" means it worked on a real VM. Say plainly what was and wasn't run.

## Live VMs

- Before any live resize, state the VM, device alias, target size, and how to
  roll back. Wait for live `current` to converge; never stack resizes.
- Ask first before reboots, VM definition changes, or anything not reversible.

## Docs

- Update `PLAN.md` when the state or plan changes. Don't create new doc files,
  task boards, roadmaps, or prompt files.
