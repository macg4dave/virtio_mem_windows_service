---
mode: agent
description: Change the Windows telemetry service
---

Change: `${input:change:required Windows service behaviour}`

- Preserve one worker constructor for console and SCM startup. Attach the SCM
  dispatcher before loading configuration and report service state accurately.
- Publish telemetry by atomic replacement. Change shared schema in core and
  update host validation together when a protocol change is required.
- Hold the condition variable's mutex when changing the cancellation predicate
  so a stop request cannot be missed. Verify the stop interleaving on Windows.
- Apply the repository's platform safety and verification rules; use
  Development for Windows build and native validation commands.

For remote build, native tests and VM diagnostics, follow
[Windows build/test prompt](rust-windows-build-test.prompt.md).
