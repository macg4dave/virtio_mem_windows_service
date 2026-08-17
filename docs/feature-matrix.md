# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
|---------|--------|-------|-----------|----------|-------|
| Read memory metrics from QEMU Guest Agent | Planned | Windows | Service | Rust | Native `guest-get-memory-stats` parser and adapter |
| Poll Windows memory availability | Planned | Windows | Service | Rust | Polling cadence to be confirmed |
| Validate virtio-mem state | Planned | Host | Ops | Bash + Rust | Inspect live XML and verify adjustment safety |
| Windows service memory collection | Planned | Windows | Service | Rust | Revisit only if native QGA is insufficient |
| QEMU Guest Agent integration | Planned | Host | Validation | Bash + Rust | Uses host `virsh qemu-agent-command` |
| Logging and metrics | Planned | Both | Both | Rust | Per-service logging |
| Error handling and recovery | Planned | Both | Both | Rust | Explicit error handling |
| Automation and scripts | Planned | Both | Ops | Bash | Build and validation helpers |

## Platform Support

- **Windows Service**: Windows 11, requires Rust 1.70+
- **Host automation**: RHEL host with Bash tooling and libvirt validation

## Language Constraints

Allowed languages are strictly limited to Rust and Bash. Go, C#, PowerShell, Python, and any other languages are not permitted in this repository.

## Configuration

All configuration currently follows the planning stage. Future work may externalize settings via TOML or environment variables and Bash automation helpers.
