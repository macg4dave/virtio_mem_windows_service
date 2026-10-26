---
mode: agent
description: Change the Windows telemetry service (windows/ crate) safely
---

Change: `${input:change:what the Windows service should do differently}`

- Windows measures and publishes telemetry only; it never resizes memory.
- Keep `unsafe` to Win32 calls with a `SAFETY` comment covering pointer
  validity, lifetimes, callbacks, and handle ownership/cleanup. Close every
  handle on every path.
- Keep one worker-construction path for console and SCM startup. Attach the
  SCM dispatcher before loading configuration; report Running/Failed accurately.
- Publish telemetry by atomic replacement. Any schema change must be made in
  `virtio-mem-core` first, with host validation updated in the same change.
- Cancellation must hold the condition variable's mutex when changing the
  predicate, so a stop request cannot be missed.
- Verify with `cargo build -p virtio-mem-service --target x86_64-pc-windows-gnu`
  (or the selected `-msvc` target) and the core/host tests. State plainly that
  native SCM and Win32 behaviour has not been run unless it was run on Windows.
