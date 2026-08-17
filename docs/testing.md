# Testing Strategy

## Local Testing

All testing is performed locally. No CI pipeline is currently configured.

### Go Testing

```bash
cd linux
go test ./...
go vet ./...
gofmt -w .
```

The controller unit tests cover QGA response parsing, threshold boundaries, minimum/maximum clamping, invalid configuration, and suppression while virtio-mem is converging. They do not require a live VM.

### Real VM validation

On the RHEL host, validate the Windows guest agent and live device before enabling automatic updates:

1. Confirm `guest-info` and `guest-get-memory-stats` succeed three times.
2. Capture the virtio-mem alias, block size, `requested`, and `current` from live XML.
3. Perform one reversible aligned live resize manually.
4. Confirm `current` converges before testing another request.
5. Test guest-agent interruption, guest reboot, failed update, and controller restart.

### Rust Service Testing

- Build locally via `cargo build --release` and `cargo test`
- Manual testing on a Windows 11 guest with QEMU Guest Agent running
- Verify memory metrics are exposed via the guest agent interface
- Validate memory change requests are processed correctly

## Validation Checklist

Before committing:

- [ ] All tests pass locally
- [ ] Code is formatted and linted
- [ ] Service boundaries are respected
- [ ] Documentation is updated
- [ ] No credentials or secrets committed

## Known Blockers

Document any local testing blockers here when encountered.
