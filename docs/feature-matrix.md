# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
| --------- | -------- | ------- | ----------- | ---------- | ------- |
| Read memory metrics from QEMU Guest Agent | In Progress | Windows | Service | Rust | Parser, bounded configurable named-pipe client, configured interactive/SCM polling worker, and explicit startup failures implemented; live QGA capability and current-allocation validation remain |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Parser, policy, adapter-based loop, stoppable scheduler, and native demand telemetry implemented; production worker wiring and workload evidence remain |
| Validate virtio-mem state | In Progress | Host | Ops | Rust | Authoritative Rust CLI provides alias-scoped snapshot/validation, checked KiB/XML boundaries, device/block alignment, exact dry-run arguments, explicit apply, convergence, compatibility, and host/device-headroom gates; live compatibility/workload evidence remains |
| RHEL virtio-mem controller | In Progress | Host | Service | Rust + systemd | One explicitly configured `win11_gpu` instance is installed and active under the least-privilege `virtio-mem-host` account; `dommemstat` fallback, device-headroom, host-memory gate, system URI, and conservative counter handling are validated; recovery and extended workload validation remain |
| Windows service memory collection | In Progress | Windows | Service | Rust | Memory stats model and threshold policy implemented |
| QEMU Guest Agent integration | In Progress | Host | Validation | Bash + Rust | Contract and parser implemented; live `virsh` validation remains |
| Service lifecycle hosting | In Progress | Windows | Service | Rust | `ServiceHost` state machine, shared stop signaling, bounded shutdown, SCM dispatcher/status callbacks, stable Event Log records, description, bounded failure actions, and install/start/stop/remove commands are locally tested; live recovery and Event Log observation remain |
| Service configuration | In Progress | Windows | Service | Rust | Versioned JSON schema, validated identity, QGA endpoint, demand-report path, timing, least-privilege account defaults, missing-file defaults, startup loading, and service recovery metadata implemented; ACL provisioning and migration policy remain |
| Logging and metrics | In Progress | Both | Both | Rust | Windows SCM lifecycle and failures emit stable, bounded Application Event Log records; live observation and broader metrics remain |
| Error handling and recovery | In Progress | Both | Both | Rust | Intentional Windows stops remain zero-exit while failures and stopless worker exits become non-zero SCM failures; live recovery remains unverified |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, Rust validation, virtio-mem inspection, read-only decision preview, and guarded reversible live-resize test helpers added; live resize remains explicitly opt-in |
| RHEL-controlled cross-platform developer gate | Complete | Both | Ops | Bash + VS Code tasks | Two fingerprint-pinned RHEL-controlled aggregate runs pass against `ice101.lan`; each includes 38 core/host tests, 59 Windows tests, native lint/build gates, and the same checksum-verified artifact |
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
