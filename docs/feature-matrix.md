# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
| --------- | -------- | ------- | ----------- | ---------- | ------- |
| Read memory metrics from QEMU Guest Agent | In Progress | Windows | Service | Rust | Parser, bounded configurable named-pipe client, configured interactive/SCM polling worker, and explicit startup failures implemented; live QGA capability and current-allocation validation remain |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Parser, policy, adapter-based loop, stoppable scheduler, and native demand telemetry implemented; production worker wiring and workload evidence remain |
| Validate virtio-mem state | In Progress | Host | Ops | Bash + Rust | Canonical byte-based VirtioMemState validation, captured libvirt XML parsing, injectable XML state provider, and requested/current convergence policy implemented; live XML discovery and resize sink remain |
| RHEL virtio-mem controller | In Progress | Host | Service | Rust + systemd | One explicitly configured VM and alias per systemd instance; bounded `virsh` QGA/XML/resize adapters, a `dommemstat`-based memory-stat fallback, a device-headroom invariant, and a host-memory-headroom gate implemented; live validation, systemd installation, and least-privilege account setup remain |
| Windows service memory collection | In Progress | Windows | Service | Rust | Memory stats model and threshold policy implemented |
| QEMU Guest Agent integration | In Progress | Host | Validation | Bash + Rust | Contract and parser implemented; live `virsh` validation remains |
| Service lifecycle hosting | In Progress | Windows | Service | Rust | `ServiceHost` state machine, shared stop signaling, bounded shutdown, SCM dispatcher/status callbacks, description, bounded failure actions, and install/start/stop/remove commands implemented and lifecycle-tested; live recovery and Event Log validation remain |
| Service configuration | In Progress | Windows | Service | Rust | Versioned JSON schema, validated identity, QGA endpoint, demand-report path, timing, least-privilege account defaults, missing-file defaults, startup loading, and service recovery metadata implemented; ACL provisioning and migration policy remain |
| Logging and metrics | Planned | Both | Both | Rust | Per-service logging |
| Error handling and recovery | In Progress | Both | Both | Rust | Explicit runtime and resize failures are covered locally; live host recovery remains blocked |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, Rust validation, virtio-mem inspection, read-only decision preview, and guarded reversible live-resize test helpers added; live resize remains explicitly opt-in |
| Native Windows memory telemetry | In Progress | Windows | Demand agent | Rust | `GlobalMemoryStatusEx` and `GetPerformanceInfo` collector implemented with checked byte conversion and deterministic validation tests; live workload evidence remains |
| Versioned Windows demand report | In Progress | Windows | Demand agent | Rust | Version 1 raw counters, bounded pressure ratios, five provisional demand states, aligned bounded target, and safe-floor recommendation implemented; remains advisory |
| Durable demand report output | In Progress | Windows | Demand agent | Rust | Validated JSON-lines publisher and generic stoppable worker implemented; ProgramData ACL setup, event-log integration, and production allocation provider remain |
| Four-level memory target model | In Progress | Both | Contract | Rust | Configured minimum, safe floor, desired target, and observed current allocation are represented and tested; cross-layer current-allocation evidence remains |
| Driver/QEMU state reconciliation | Research | Both | Integration | Rust + Bash | Validate `requested_size`/`plugged_size` against libvirt `requested`/`current`; no direct IOCTL assumed |
| Global VM pool accounting | Planned | Host | Global controller | Rust | Host reserve and actual observed VM allocations; Phase 3 |
| Growth and reclaim priorities | Planned | Host | Global controller | Rust | Separate per-VM growth and reclaim priority; Phase 3 |
| Trend-aware safe reclaim | Planned | Both | Policy | Rust | Rolling history, safe floors, bounded aligned steps, and convergence gates; Phase 3 |
| viomem user-mode interface | Deferred | Windows | Driver research | Rust + upstream driver | Device interface exists upstream, but a supported user-mode status/IOCTL contract is unverified |

## Platform Support

- **Windows Service**: Windows 11, requires Rust 1.70+
- **Host automation**: RHEL host with Bash tooling and libvirt validation

## Language Constraints

Allowed languages are strictly limited to Rust and Bash. Go, C#, PowerShell, Python, and any other languages are not permitted in this repository.

## Configuration

Configuration uses validated versioned JSON at `C:\ProgramData\VirtioMemService\config.json`, with validated defaults when the file is absent. ACL provisioning, migration policy, and production installation remain open.
