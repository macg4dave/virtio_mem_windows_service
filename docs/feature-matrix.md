# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
| --------- | -------- | ------- | ----------- | ---------- | ------- |
| Legacy QGA memory adapter | Complete | Windows | Adapter/test boundary | Rust | Framing, parsing, deadlines, cancellation, and errors are tested; production workers use native telemetry and do not open the QGA-owned device |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Native telemetry and stoppable worker are implemented; production publication, current-allocation ownership, and workload evidence remain |
| Validate virtio-mem state | Complete | Host | Ops | Rust | Live authoritative Rust CLI snapshot/validation passes for `ua-virtiomem0`, including canonical fully-unplugged state and wrong-alias rejection; actuation compatibility/workload evidence remains separately tracked by M9a |
| RHEL virtio-mem controller | Complete | Host | Service | Rust + systemd | Installed controller completed a guarded zero-to-1-GiB bootstrap on `win11_gpu`, converged and retained its minimum; fresh XML/QMP, `dommemstat`, device/host headroom, THP, workload, and no-overlap gates passed |
| Windows service memory collection | Complete | Windows | Service | Rust | Interactive and SCM paths collect and validate `GlobalMemoryStatusEx`/`GetPerformanceInfo` telemetry; publication is tracked separately |
| QEMU Guest Agent integration | Complete | Host | Validation | Bash + Rust | Repeated live QGA and `dommemstat` probes passed before and after isolated QGA restart and graceful guest reboot; QGA memory stats are unsupported, so the verified fallback remains authoritative |
| Service lifecycle hosting | Complete | Windows | Service | Rust | Elevated LocalService install/start/observe/stop/delete passed; raw Event Log transitions, clean exit, non-zero failure, and the first 5-second recovery restart were observed live |
| Service configuration | In Progress | Windows | Service | Rust | ProgramData versioned JSON, schema/basic validation, defaults, startup loading, and recovery metadata are implemented; production ACLs, atomic updates, migration, and stronger account/path/duration bounds remain |
| Logging and metrics | In Progress | Both | Both | Rust | Windows SCM lifecycle and failure records are live-verified in XML EventData; classic text descriptions need message-resource packaging and broader metrics remain |
| Error handling and recovery | In Progress | Both | Both | Rust | Intentional Windows stop is live-verified at exit zero; invalid configuration exits one and triggered the configured 5-second recovery restart; host recovery remains |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, Rust validation, virtio-mem inspection, read-only decision preview, and guarded reversible live-resize test helpers added; live resize remains explicitly opt-in |
| RHEL-controlled cross-platform developer gate | Complete | Both | Ops | Bash + VS Code tasks | Historical repeatability gate passed 38 core/host and 59 Windows tests; latest platform gates pass 22 core, 29 host, and 64 Windows tests with native lint/build checks |
| Native Windows memory telemetry | In Progress | Windows | Demand agent | Rust | `GlobalMemoryStatusEx` and `GetPerformanceInfo` collector implemented with checked byte conversion and deterministic validation tests; live workload evidence remains |
| Versioned Windows demand report | In Progress | Windows | Demand agent | Rust | Version 1 raw counters, bounded pressure ratios, five provisional demand states, aligned bounded target, and safe-floor recommendation implemented; remains advisory |
| Demand report output | In Progress | Windows | Demand agent | Rust | Append-only JSON-lines publisher and generic worker are tested; production allocation ownership, identity/freshness envelope, ACLs, partial-record handling, retention, and rotation remain |
| Four-level memory target model | In Progress | Both | Contract | Rust | Configured minimum, safe floor, desired target, and observed current allocation are represented and tested; cross-layer current-allocation evidence remains |
| Virtio-mem compatibility gate | Complete | Host | Adapter | Rust | Fresh selected-device QMP confirms both required properties; THP, VFIO device classes, `mem-lock=off`, RDMA/vhost-user absence, explicit workload review, exact dry run, and non-mutation passed live |
| Compatibility-attestation drift guard | Planned | Host | Safety | Rust | M9d will bind operator review to a live domain/QEMU fingerprint and fail closed after configuration drift |
| Driver/QEMU state reconciliation | Blocked | Both | Integration | Rust + Bash | M10a1–M10a4 now separate capture qualification, evidence harness, one-block mapping, and contract adoption; M10aX is conditional only if bounded debug capture fails |
| Current-allocation/report join | Planned | Both | Contract | Rust | M10c selects a host join or validated allocation feed without granting Windows host-control authority |
| Report freshness and bounded delivery | Planned | Both | Contract | Rust | M10d adds identity, timestamps, session/sequence, provenance, replay rules, ACLs, partial-record handling, retention, and rotation |
| Single-VM recovery matrix | Planned | Host | Integration | Rust + Bash | M10b covers rejection, timeout, non-convergence, interruption, reboot, cancellation, and restart without replay or overlap |
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
