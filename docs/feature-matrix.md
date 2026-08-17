# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
|---------|--------|-------|-----------|----------|-------|
| Read memory metrics from QEMU Guest Agent | Implemented (foundation) | Linux | Controller | Go | Native `guest-get-memory-stats` parser and adapter |
| Poll Windows memory availability | Implemented (foundation) | Linux | Controller | Go | 10s default interval |
| Calculate hysteresis-based allocation | Implemented (foundation) | Linux | Controller | Go | 2GB thresholds and convergence guard |
| Execute virtio-mem live update | Implemented (foundation) | Linux | Controller | Go | Live XML state and 8-28 GB clamp |
| Windows service memory collection | Deferred | Windows | Service | Rust | Revisit only if native QGA is insufficient |
| QEMU Guest Agent integration | Implemented (host path) | Linux | Controller | Go | Uses host `virsh qemu-agent-command` |
| Logging and metrics | Planned | Both | Both | Go/Rust | Per-service |
| Error handling and recovery | Planned | Both | Both | Go/Rust | Explicit error handling |
| Automation and scripts | Planned | Both | Ops | Bash | Build and validation helpers |

## Platform Support

- **Linux Controller**: RHEL, requires Go 1.20+
- **Windows Service**: Windows 11, requires Rust 1.70+

## Language Constraints

Allowed languages are strictly limited to Go, Rust, and Bash. C#, PowerShell, Python, and any other languages are not permitted in this repository.

## Configuration

All configuration currently hardcoded. Future: externalize via TOML or environment variables and Bash automation helpers.
