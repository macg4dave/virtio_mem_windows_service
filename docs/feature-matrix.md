# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
|---------|--------|-------|-----------|----------|-------|
| Read memory metrics from QEMU Guest Agent | In Progress | Windows | Service | Rust | Parser and configurable named-pipe client implemented; service scheduling remains |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Parser, policy, adapter-based loop, and stoppable interval scheduler implemented; service hosting remains |
| Validate virtio-mem state | In Progress | Host | Ops | Bash + Rust | Canonical byte-based VirtioMemState validation, captured libvirt XML parsing, injectable XML state provider, and requested/current convergence policy implemented; live XML discovery and resize sink remain |
| RHEL virtio-mem controller | In Progress | Host | Service | Rust + systemd | One explicitly configured VM and alias per systemd instance; bounded `virsh` QGA/XML/resize adapters and live validation are in progress |
| Windows service memory collection | In Progress | Windows | Service | Rust | Memory stats model and threshold policy implemented |
| QEMU Guest Agent integration | In Progress | Host | Validation | Bash + Rust | Contract and parser implemented; live `virsh` validation remains |
| Service lifecycle hosting | In Progress | Windows | Service | Rust | `ServiceHost` state machine, shared stop signaling, SCM dispatcher/status callbacks, and install/start/stop/remove commands implemented; live Windows service registration remains |
| Service configuration | In Progress | Windows | Service | Rust | Validated identity, QGA endpoint, timing, least-privilege account defaults, and startup guard implemented; persistence remains |
| Logging and metrics | Planned | Both | Both | Rust | Per-service logging |
| Error handling and recovery | In Progress | Both | Both | Rust | Explicit runtime and resize failures are covered locally; live host recovery remains blocked |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, and Rust validation helpers added |

## Platform Support

- **Windows Service**: Windows 11, requires Rust 1.70+
- **Host automation**: RHEL host with Bash tooling and libvirt validation

## Language Constraints

Allowed languages are strictly limited to Rust and Bash. Go, C#, PowerShell, Python, and any other languages are not permitted in this repository.

## Configuration

All configuration currently follows the planning stage. Future work may externalize settings via TOML or environment variables and Bash automation helpers.
