# Feature Matrix

| Feature | Status | Owner | Component | Language | Notes |
| --------- | -------- | ------- | ----------- | ---------- | ------- |
| Custom guest-agent memory adapter | Experimental | Windows | Adapter/test boundary | Rust | `guest-get-memory-stats` is not upstream QGA; framing, parsing, deadlines, cancellation, and errors are tested only for an exact custom/downstream implementation, while production workers use native telemetry |
| Poll Windows memory availability | In Progress | Windows | Service | Rust | Native telemetry and stoppable raw publication worker are implemented; workload evidence and M10d delivery hardening remain |
| Validate virtio-mem state | Complete | Host | Ops | Rust | Live authoritative Rust CLI snapshot/validation passes for `ua-virtiomem0`, including canonical fully-unplugged state and wrong-alias rejection; actuation compatibility/workload evidence remains separately tracked by M9a |
| RHEL virtio-mem controller | In Progress | Host | Service | Rust + systemd | Installed controller completed a guarded zero-to-1-GiB bootstrap on trusted development guest `win11_gpu`; M9d, M9e, M10b, and the M10e estimator are complete, while M10f-M10g reconciliation and qualification remain |
| Windows service memory collection | Complete | Windows | Service | Rust | Interactive and SCM paths collect and validate `GlobalMemoryStatusEx`/`GetPerformanceInfo` telemetry; publication is tracked separately |
| QEMU Guest Agent integration | Complete | Host | Health/identity validation | Bash + Rust | Repeated advertised QGA commands passed across restart/reboot; upstream QGA has no `guest-get-memory-stats`, and production Windows telemetry is independent |
| Host memory-stat correctness and freshness | Complete | Host | Adapter/policy | Rust | Balloon `actual` is provenance only; required `unused`/`available` and bounded advancing `last-update` feed the same Rust decision path as the controller |
| Service lifecycle hosting | Complete | Windows | Service | Rust | Elevated LocalService install/start/observe/stop/delete passed; raw Event Log transitions, clean exit, non-zero failure, and the first 5-second recovery restart were observed live |
| Service configuration | In Progress | Windows | Service | Rust | ProgramData versioned JSON, schema/basic validation, defaults, startup loading, and recovery metadata are implemented; production ACLs, atomic updates, migration, and stronger account/path/duration bounds remain |
| Logging and metrics | In Progress | Both | Both | Rust | Windows SCM lifecycle and failure records are live-verified in XML EventData; classic text descriptions need message-resource packaging and broader metrics remain |
| Error handling and recovery | In Progress | Both | Both | Rust | Intentional Windows stop is live-verified at exit zero; invalid configuration exits one and triggered the configured 5-second recovery restart; host recovery remains |
| Automation and scripts | In Progress | Both | Ops | Bash | Prerequisite, QGA probe, Rust validation, virtio-mem inspection, read-only decision preview, and guarded reversible live-resize test helpers added; live resize remains explicitly opt-in |
| RHEL-controlled cross-platform developer gate | Complete | Both | Ops | Bash + VS Code tasks | Latest gates pass 55 core, 60 host, and 67 native Windows tests with release builds, formatting, and warnings-as-errors Clippy |
| Native Windows memory telemetry | In Progress | Windows | Demand agent | Rust | `GlobalMemoryStatusEx` and `GetPerformanceInfo` collector implemented with checked byte conversion and deterministic validation tests; live workload evidence remains |
| Versioned Windows demand report | In Progress | Windows | Demand agent | Rust | Version 2 raw telemetry carries VM/service/session identity, dual clocks, sequence, and explicit native-source/host-join provenance; the separate calculated report remains advisory |
| Demand report output | In Progress | Windows | Demand agent | Rust | Production atomically replaces one allocation-free record with three retained copies; the host persists restart-safe replay acknowledgement and enforces identity plus size bounds; native ProgramData ACL installation remains to be verified |
| Four-level memory target model | In Progress | Both | Contract | Rust | Configured minimum, safe floor, desired target, and authoritative alias-scoped live libvirt current allocation are joined and calculated on the host; workload tuning remains |
| Quantitative desired-allocation model | Complete | Host policy | Contract | Rust | M10e implements checked physical/commit candidates, fixed-visible-base validation, effective maximum, reserve defaults, bounded 10-minute high-water history, 256 MiB hysteresis, safe floors, capacity-limited output, and fingerprint-bound atomic restart state |
| Desired/requested/current reconciliation | Planned | Host | Controller | Rust | M10f preserves current as authority, journals intent, safely raises/cancels pending shrink, freezes owned shrink on stale telemetry, prohibits lower overlap, persists latches, and exposes constrained progress |
| Virtio-mem compatibility gate | Complete | Host | Adapter | Rust | Version-1 SHA-256 attestation binds backend, slot/mapping, balloon, incompatible-workload, topology, trust, driver, QEMU, and libvirt review to fresh live evidence |
| Compatibility-attestation drift guard | Complete | Host | Safety | Rust | Every resize verifies attestation integrity, recollects alias-scoped live evidence, ignores only allocation progress, and fails closed on configuration or version drift |
| Allocation-authority reconciliation | Complete | Host | Contract | Rust | Virtio 1.2 and pinned QEMU/libvirt/virtio-win sources establish requested/plugged semantics; live libvirt `current` is authoritative and driver trace is optional diagnostic evidence |
| Installed-driver diagnostic tracing | Optional | Windows | Integration | Bash | Checksum/signature-verified bounded DbgView captures completed without driver restart, boot logging, or debug-filter changes; no matching informational record appeared during either no-resize qualification or the M10a3 one-block operation |
| Correlated behavior evidence | Complete | Both | Contract/test boundary | Rust | Version 1 requires repeated operation/VM/device identity, explicit bytes, source identity, ordered clocks, converged host endpoints, Windows health, and controller state; driver trace is optional and never allocation authority |
| Current-allocation/report join | Complete | Both | Contract | Rust | M10c publishes raw Windows telemetry and joins its validated VM/time envelope with alias-scoped live libvirt `current` on the host; Windows receives no host-control authority or allocation feed |
| Report freshness and bounded delivery | In Progress | Both | Contract | Rust | Version 2 identity/provenance, atomic handoff/retention, durable replay state, partial/oversized rejection, and ProgramData ACL provisioning are implemented; installed ACL verification remains |
| Windows automatic shrink | Implemented; default enabled | Host + driver | Integration | Rust + Bash | Reclaim is a core default-on capability with explicit disable override, bounded 64 MiB requests, default-off re-notification, convergence and ambiguity/stall latching; current live evidence remains zero/partial progress |
| Single-VM recovery matrix | Complete | Host | Integration | Rust + Bash | M10b closed with hermetic cancellation, restart, interruption, ownership-conflict, typed preflight/unknown-command, stall-latch, and no-replay coverage plus installed rejection/no-replay, bounded retry, one-shot recovery, and partial/no-progress live evidence |
| Multi-controller/device actuation | Deferred | Host | Global controller | Rust | Upstream supports multiple devices, but Phase 2 permits one active controller/device on this development host until M11 provides atomic global reservation |
| Global VM pool accounting | Planned | Host | Global controller | Rust | Host reserve and actual observed VM allocations; Phase 3 |
| Growth and reclaim priorities | Planned | Host | Global controller | Rust | Separate per-VM growth and reclaim priority; Phase 3 |
| Trend-aware safe reclaim | Planned | Both | Policy | Rust | M10e-M10g define and qualify absolute targets, rolling history, quantitative reserves, safe floors, upward supersession, bounded aligned actuation, and constrained-current handling before Phase 3 reuse |
| viomem user-mode interface | Feasibility complete; implementation deferred | Windows | Driver research | Rust + upstream driver | M10aX specifies a versioned cached read-only diagnostic IOCTL with administrator/SYSTEM ACL, hostile-input/concurrency tests, external signing/install, and rollback gates; closed M10b evidence did not justify implementation, which remains No-Go without external driver ownership and the other proposal gates |

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
