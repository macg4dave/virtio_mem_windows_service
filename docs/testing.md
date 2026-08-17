# Testing Strategy

## Local Testing

All testing is performed locally. No CI pipeline is currently configured.

See [`dependencies.md`](dependencies.md) for the complete toolchain and host/
guest prerequisite matrix.

### Rust Service Testing

```bash
cd windows
cargo build --release
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
```

The Rust service tests should cover QGA response parsing, threshold boundaries, minimum/maximum safe ranges, and invalid configuration cases. They do not require a live VM.

### Windows service lifecycle testing

The SCM adapter and portable host must be tested separately from live QEMU
Guest Agent access. At minimum, verify:

1. startup reports pending before initialization and running only after the
  worker is ready;
2. a stop request cancels scheduling, prevents new polls, and reaches stopped;
3. system shutdown follows the same graceful cancellation path;
4. repeated stop requests are harmless and do not deadlock;
5. an unexpected worker error reaches failed and produces a non-zero process
  result for SCM recovery; and
6. a slow stop reports stop-pending and completes within the configured
  shutdown deadline.

The portable Rust tests additionally verify startup failure before `Running`,
atomic stop signaling, cancellation wake-up, configuration validation, and
that a stopped loop performs no new poll.

Installation validation must also confirm the registered service name,
description, executable path, account, startup mode, and recovery actions.
Verify the sequence **install → start → observe logs → stop → delete** on a
Windows test machine. Do not test recovery by terminating a production
service or by using unbounded restart loops.

### Real VM validation

On the RHEL host, validate the Windows guest agent and live device before enabling automatic updates:

1. Confirm `guest-info` and `guest-get-memory-stats` succeed three times.
2. Capture the virtio-mem alias, block size, `requested`, and `current` values from live XML.
3. Perform one reversible aligned live resize manually.
4. Confirm `current` converges before testing another request.
5. Test guest-agent interruption, guest reboot, failed update, and service restart.

### Bash validation helpers

- Run focused shell validation scripts locally before use on a target host.
- Check for required environment variables and host tooling early.
- Prefer explicit error handling and exit codes over silent fallback behavior.

## Validation Checklist

Before committing:

- [ ] All tests pass locally
- [ ] Code is formatted and linted
- [ ] Service boundaries are respected
- [ ] Documentation is updated
- [ ] No credentials or secrets committed

## Known Blockers

- The Windows-native Rust 1.97.1 MSVC toolchain now passes the full local
  format, release build, test, and Clippy pipeline.
- Live QEMU Guest Agent and libvirt validation requires the RHEL host and
  Windows guest described in `docs/qemu-ga-setup.md`.
