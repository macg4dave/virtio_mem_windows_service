# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
| --------- | -------- | ------- | ----------- | ---------- | ------- |
| Read memory metrics from QEMU Guest Agent | In Progress | Windows | Service | Rust | Parser, bounded configurable named-pipe client, configured interactive/SCM polling worker, and explicit startup failures implemented; live QGA capability and current-allocation validation remain |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Parser, policy, adapter-based loop, stoppable scheduler, and native demand telemetry implemented; production worker wiring and workload evidence remain |
| Validate virtio-mem state | Complete | Host | Ops | Rust | Live authoritative Rust CLI snapshot/validation passes for `ua-virtiomem0`, including canonical fully-unplugged state and wrong-alias rejection; actuation compatibility/workload evidence remains separately tracked by M9a |
| RHEL virtio-mem controller | In Progress | Host | Service | Rust + systemd | Live XML/QMP evidence, `dommemstat`, device/host headroom, THP matching, explicit workload review, and incompatible-class checks are validated on `win11_gpu`; installed-controller recovery and applied convergence remain |
| Windows service memory collection | In Progress | Windows | Service | Rust | Memory stats model and threshold policy implemented |
| QEMU Guest Agent integration | Complete | Host | Validation | Bash + Rust | Repeated live QGA and `dommemstat` probes passed before and after isolated QGA restart and graceful guest reboot; QGA memory stats are unsupported, so the verified fallback remains authoritative |
| Service lifecycle hosting | Complete | Windows | Service | Rust | Elevated LocalService install/start/observe/stop/delete passed; raw Event Log transitions, clean exit, non-zero failure, and the first 5-second recovery restart were observed live |
| Service configuration | In Progress | Windows | Service | Rust | Versioned JSON schema, validated identity, QGA endpoint, demand-report path, timing, least-privilege account defaults, missing-file defaults, startup loading, and service recovery metadata implemented; ACL provisioning and migration policy remain |
| Logging and metrics | In Progress | Both | Both | Rust | Windows SCM lifecycle and failure records are live-verified in XML EventData; classic text descriptions need message-resource packaging and broader metrics remain |
| Error handling and recovery | In Progress | Both | Both | Rust | Intentional Windows stop is live-verified at exit zero; invalid configuration exits one and triggered the configured 5-second recovery restart; host recovery remains |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, Rust validation, virtio-mem inspection, read-only decision preview, and guarded reversible live-resize test helpers added; live resize remains explicitly opt-in |
| RHEL-controlled cross-platform developer gate | Complete | Both | Ops | Bash + VS Code tasks | Two fingerprint-pinned RHEL-controlled aggregate runs pass against `ice101.lan`; each includes 38 core/host tests, 59 Windows tests, native lint/build gates, and the same checksum-verified artifact |
| Native Windows memory telemetry | In Progress | Windows | Demand agent | Rust | `GlobalMemoryStatusEx` and `GetPerformanceInfo` collector implemented with checked byte conversion and deterministic validation tests; live workload evidence remains |
| Versioned Windows demand report | In Progress | Windows | Demand agent | Rust | Version 1 raw counters, bounded pressure ratios, five provisional demand states, aligned bounded target, and safe-floor recommendation implemented; remains advisory |
| Durable demand report output | In Progress | Windows | Demand agent | Rust | Validated JSON-lines publisher and generic stoppable worker implemented; ProgramData ACL setup, event-log integration, and production allocation provider remain |
| Four-level memory target model | In Progress | Both | Contract | Rust | Configured minimum, safe floor, desired target, and observed current allocation are represented and tested; cross-layer current-allocation evidence remains |
| Virtio-mem compatibility gate | Complete | Host | Adapter | Rust | Fresh selected-device QMP confirms both required properties; THP, VFIO device classes, `mem-lock=off`, RDMA/vhost-user absence, explicit workload review, exact dry run, and non-mutation passed live |
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
