# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
| --------- | -------- | ------- | ----------- | ---------- | ------- |
| Custom guest-agent memory adapter | Experimental | Windows | Adapter/test boundary | Rust | `guest-get-memory-stats` is not upstream QGA; framing, parsing, deadlines, cancellation, and errors are tested only for an exact custom/downstream implementation, while production workers use native telemetry |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Native telemetry and stoppable worker are implemented; production publication, current-allocation ownership, and workload evidence remain |
| Validate virtio-mem state | Complete | Host | Ops | Rust | Live authoritative Rust CLI snapshot/validation passes for `ua-virtiomem0`, including canonical fully-unplugged state and wrong-alias rejection; actuation compatibility/workload evidence remains separately tracked by M9a |
| RHEL virtio-mem controller | In Progress | Host | Service | Rust + systemd | Installed controller completed a guarded zero-to-1-GiB bootstrap on trusted development guest `win11_gpu`; M9d full attestation, M9e stats freshness/semantics, and M10b shrink/recovery qualification remain |
| Windows service memory collection | Complete | Windows | Service | Rust | Interactive and SCM paths collect and validate `GlobalMemoryStatusEx`/`GetPerformanceInfo` telemetry; publication is tracked separately |
| QEMU Guest Agent integration | Complete | Host | Health/identity validation | Bash + Rust | Repeated advertised QGA commands passed across restart/reboot; upstream QGA has no `guest-get-memory-stats`, and production Windows telemetry is independent |
| Host memory-stat correctness and freshness | Planned | Host | Adapter/policy | Rust | M9e corrects `dommemstat actual` balloon semantics, validates `last-update`, and replaces the QGA-only Bash preview with the controller source path |
| Service lifecycle hosting | Complete | Windows | Service | Rust | Elevated LocalService install/start/observe/stop/delete passed; raw Event Log transitions, clean exit, non-zero failure, and the first 5-second recovery restart were observed live |
| Service configuration | In Progress | Windows | Service | Rust | ProgramData versioned JSON, schema/basic validation, defaults, startup loading, and recovery metadata are implemented; production ACLs, atomic updates, migration, and stronger account/path/duration bounds remain |
| Logging and metrics | In Progress | Both | Both | Rust | Windows SCM lifecycle and failure records are live-verified in XML EventData; classic text descriptions need message-resource packaging and broader metrics remain |
| Error handling and recovery | In Progress | Both | Both | Rust | Intentional Windows stop is live-verified at exit zero; invalid configuration exits one and triggered the configured 5-second recovery restart; host recovery remains |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, Rust validation, virtio-mem inspection, read-only decision preview, and guarded reversible live-resize test helpers added; live resize remains explicitly opt-in |
| RHEL-controlled cross-platform developer gate | Complete | Both | Ops | Bash + VS Code tasks | Historical repeatability gate passed 38 core/host and 59 Windows tests; latest platform gates pass 22 core, 29 host, and 64 Windows tests with native lint/build checks |
| Native Windows memory telemetry | In Progress | Windows | Demand agent | Rust | `GlobalMemoryStatusEx` and `GetPerformanceInfo` collector implemented with checked byte conversion and deterministic validation tests; live workload evidence remains |
| Versioned Windows demand report | In Progress | Windows | Demand agent | Rust | Version 1 raw counters, bounded pressure ratios, five provisional demand states, aligned bounded target, and safe-floor recommendation implemented; remains advisory |
| Demand report output | In Progress | Windows | Demand agent | Rust | Append-only JSON-lines publisher and generic worker are tested; production allocation ownership, identity/freshness envelope, ACLs, partial-record handling, retention, and rotation remain |
| Four-level memory target model | In Progress | Both | Contract | Rust | Configured minimum, safe floor, desired target, and authoritative alias-scoped live libvirt current allocation are represented and tested; production report join remains |
| Virtio-mem compatibility gate | In Progress | Host | Adapter | Rust | Initial trusted `win11_gpu` QMP/THP/VFIO/workload evidence passed; M9d must fingerprint complete backend, slot/mapping, balloon, incompatible-workload, topology, trust, and version evidence |
| Compatibility-attestation drift guard | Planned | Host | Safety | Rust | M9d binds the complete review to a live domain/QEMU fingerprint and fails closed after configuration drift |
| Allocation-authority reconciliation | Complete | Host | Contract | Rust | Virtio 1.2 and pinned QEMU/libvirt/virtio-win sources establish requested/plugged semantics; live libvirt `current` is authoritative and driver trace is optional diagnostic evidence |
| Installed-driver diagnostic tracing | Optional | Windows | Integration | Bash | Checksum/signature-verified bounded DbgView captures completed without driver restart, boot logging, or debug-filter changes; no matching informational record appeared during either no-resize qualification or the M10a3 one-block operation |
| Correlated behavior evidence | Complete | Both | Contract/test boundary | Rust | Version 1 requires repeated operation/VM/device identity, explicit bytes, source identity, ordered clocks, converged host endpoints, Windows health, and controller state; driver trace is optional and never allocation authority |
| Current-allocation/report join | Planned | Both | Contract | Rust | M10c implements the selected host-side join of fresh raw Windows telemetry with alias-scoped live libvirt `current`; Windows receives no host-control authority or allocation feed |
| Report freshness and bounded delivery | Planned | Both | Contract | Rust | M10d adds identity, timestamps, session/sequence, provenance, replay rules, ACLs, partial-record handling, retention, and rotation |
| Windows automatic shrink | Unqualified; bounded policy selected | Host + driver | Integration | Rust + Bash | A one-block shrink made no progress; a 1 GiB shrink unplugged 257/512 blocks then stalled. Separate default-off shrink/re-notification controls must remain off until M10b proves exact-target notifications at 30/60/120 seconds within three retries and 300 seconds, plus one-shot abandon-to-current recovery |
| Single-VM recovery matrix | Planned | Host | Integration | Rust + Bash | M10b covers rejection, timeout, non-convergence, Windows shrink progress/recovery, interruption, reboot, cancellation, and restart without replay or overlap |
| Multi-controller/device actuation | Deferred | Host | Global controller | Rust | Upstream supports multiple devices, but Phase 2 permits one active controller/device on this development host until M11 provides atomic global reservation |
| Global VM pool accounting | Planned | Host | Global controller | Rust | Host reserve and actual observed VM allocations; Phase 3 |
| Growth and reclaim priorities | Planned | Host | Global controller | Rust | Separate per-VM growth and reclaim priority; Phase 3 |
| Trend-aware safe reclaim | Planned | Both | Policy | Rust | Rolling history, safe floors, bounded aligned steps, and convergence gates; Phase 3 |
| viomem user-mode interface | Deferred | Windows | Driver research | Rust + upstream driver | Device interface exists upstream, but a supported user-mode status/IOCTL contract is unverified |

## Platform Support

- **Windows Service**: Windows 11 x64 technology preview; requires Rust 1.70+
- **Validated guest**: fully trusted development/test KVM guest `win11_gpu`;
  no production or untrusted-guest support claim
- **Host automation**: RHEL host with Bash tooling and libvirt validation;
  hard QEMU/libvirt memory limit recommended for `win11_gpu` and mandatory for
  future untrusted/production guests

## Language Constraints

Allowed languages are strictly limited to Rust and Bash. Go, C#, PowerShell, Python, and any other languages are not permitted in this repository.

## Configuration

Configuration uses validated versioned JSON at `C:\ProgramData\VirtioMemService\config.json`, with validated defaults when the file is absent. ACL provisioning, migration policy, and production installation remain open.
