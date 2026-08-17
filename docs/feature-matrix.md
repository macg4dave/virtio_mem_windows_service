# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
|---------|--------|-------|-----------|----------|-------|
| Read memory metrics from QEMU Guest Agent | In Progress | Windows | Service | Rust | `guest-get-memory-stats` response parser implemented; transport adapter remains |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Pure decision policy implemented; timer and Windows service loop remain |
| Validate virtio-mem state | In Progress | Host | Ops | Bash + Rust | Safe requested/current convergence policy implemented; live XML adapter remains |
| Windows service memory collection | In Progress | Windows | Service | Rust | Memory stats model and threshold policy implemented |
| QEMU Guest Agent integration | In Progress | Host | Validation | Bash + Rust | Contract and parser implemented; live `virsh` validation remains |
| Logging and metrics | Planned | Both | Both | Rust | Per-service logging |
| Error handling and recovery | Planned | Both | Both | Rust | Explicit error handling |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, and Rust validation helpers added |

## Platform Support

- **Windows Service**: Windows 11, requires Rust 1.70+
- **Host automation**: RHEL host with Bash tooling and libvirt validation

## Language Constraints

Allowed languages are strictly limited to Rust and Bash. Go, C#, PowerShell, Python, and any other languages are not permitted in this repository.

## Configuration

All configuration currently follows the planning stage. Future work may externalize settings via TOML or environment variables and Bash automation helpers.
