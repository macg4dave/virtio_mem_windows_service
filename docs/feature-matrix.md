# Feature Matrix

| Capability | Status | Authority and remaining work |
| --- | --- | --- |
| Windows basic memory measurement | Implemented | Windows measures physical and commit state; it has no resize authority. |
| Windows-native pressure telemetry | Planned | Add memory-resource notifications, reusable/modified lists, paging evidence, and explicit capability state; validate on the supported native build before policy use. |
| Atomic raw telemetry delivery | Implemented | Identity, provenance, ordering, retention, size bounds, replay acknowledgement, and fail-closed transport-versus-evidence failure handling are enforced. Installed ACL/publication evidence remains. |
| Windows SCM lifecycle | Implemented | Required configuration, identity propagation, cancellation, EventData, and explicit failures are covered; packaged classic message text remains open. |
| QGA integration | Implemented health boundary | Maintained use is guest health and identity. The custom memory command is experimental and not a production dependency. |
| Live virtio-mem state | Implemented | The host selects one explicit alias; live libvirt `current` is allocation authority. |
| Compatibility and headroom gates | Implemented | Fresh attestation, device bounds, host reserve, freshness, convergence, and ownership fail closed before mutation. |
| Fixed-headroom desired target | Implemented compatibility baseline | Physical/commit candidates and history work, but fixed reserves are not a Windows demand prediction and will become fallback guards. |
| Windows-native pressure telemetry | Notification collection implemented; workload qualification pending | Schema v3 carries fixed-size capabilities and explicit fallback. The service owns paired low/high notification handles and publishes low, neutral, high, or failed state; reusable/rate collection remains planned. |
| Pressure-aware desired target | Design complete; implementation planned | The host will combine Windows-native pressure classification with a validated quantitative baseline while retaining all safety and reconciliation gates. |
| Desired/requested/current reconciliation | Implemented | Intent journaling, partial progress, upward supersession, uncertain results, restart recovery, and latches prohibit blind replay. |
| Rust build/test tooling | Implemented | `cargo xtask` is authoritative for maintained local, Windows, host, live-resize, and qualification workflows. |
| Single-VM live qualification | Previous policy paused | Prior fixed-headroom evidence remains historical; pressure semantics, shadow decisions, applied growth/reclaim, recovery, and endurance require new gates. |
| Multi-VM global pool | Prototype only | Side-effect-free arbitration exists in the shared core; durable host-wide reservation, runtime integration, and live qualification remain production-scope gates under WN10. |
| Driver status interface | Deferred | Optional diagnostics cannot become allocation authority; implementation requires separate external driver ownership and approval. |

## Platform boundary

The current support target is one fully trusted Windows development guest on a
libvirt/KVM host. Windows virtio-mem remains technology-preview integration;
there is no production or untrusted-guest support claim.

## Configuration

The Windows service requires a validated versioned configuration file. Host
policy and deployment values are validated configuration, not prompt or script
defaults. Live-test parameters are explicit xtask arguments recorded with each
run. See [testing.md](testing.md) and
[target-controller.md](target-controller.md).

## Language boundary

Runtime and maintained workflow logic are Rust. Bash is limited to a thin
cross-machine task boundary where required by the native Windows gate. Other
languages and generated project artefacts are not maintained in this
repository.
