# Feature Matrix

| Capability | Status | Authority and remaining work |
| --- | --- | --- |
| Windows basic memory measurement | Implemented | Windows measures physical and commit state; it has no resize authority. |
| Atomic raw telemetry delivery | Implemented | Identity, provenance, ordering, retention, size bounds, replay acknowledgement, and fail-closed transport-versus-evidence failure handling are enforced. Installed ACL/publication evidence remains. |
| Windows SCM lifecycle | Implemented | Required configuration, identity propagation, cancellation, EventData, and explicit failures are covered; packaged classic message text remains open. |
| QGA integration | Implemented health boundary | Maintained use is guest health and identity. The custom memory command is experimental and not a production dependency. |
| Live virtio-mem state | Implemented | The host selects one explicit alias; live libvirt `current` is allocation authority. |
| Compatibility and headroom gates | Implemented | Fresh attestation, device bounds, host reserve, freshness, convergence, and ownership fail closed before mutation. |
| Fixed-headroom desired target | Implemented temporary baseline | Physical/commit candidates and history work, but fixed reserves are not a Windows demand prediction. The estimator/configuration is removed after replacement qualification and its rollback window. |
| Windows-native pressure telemetry | Notification collection implemented; broader qualification deferred | Schema v3 carries fixed-size capabilities and explicit fallback. The native API and installed service pass their current gates; broader workload correlation moves to QA-T031 and is not release evidence. |
| Pressure-aware demand target | WN4 source integration in progress | Versioned requirement, notification-led pressure, shrink safety, history, and append-only comparison evidence are wired to accepted telemetry and alias-scoped live state. Explicit `growth` mode selects bounded normal, urgent, fallback, held, and capacity-limited growth and rejects every lower request before the sink. Native, deployment, and QA-T028 applied evidence remain outstanding. |
| Desired/requested/current reconciliation | Implemented | Intent journaling, partial progress, upward supersession, uncertain results, restart recovery, and latches prohibit blind replay. |
| Versioned controller status | Version 3 implemented; refreshed live read deferred | The read-only host command joins fresh alias-scoped live state with the matching durable checkpoint and exposes the latest pressure assessment and growth decision alongside telemetry, reclaim, capacity, ownership, latch, and recovery without mutation. The earlier version-1 live check preserved the inactive, disabled service state. |
| Rust build/test tooling | Implemented | `cargo xtask` is authoritative for maintained local, Windows, host, live-resize, and qualification workflows. |
| Single-VM live qualification | Previous policy paused | Prior fixed-headroom evidence remains historical; pressure semantics, shadow decisions, applied growth/reclaim, recovery, and endurance require new gates. |
| Host-wide VM RAM pool | HPM1 durable accounting implemented | Versioned semantic pool/member policy, OS-neutral demand reports, total-RAM charging/conversion, deterministic pure grants, and a bounded checksum-protected atomic ledger are implemented. The ledger reserves growth before dispatch, retains command ownership across restart without replay, and counts reclaim only after observed release. Final deployment syntax, exclusive coordinator, runtime integration, and later HPM qualification remain. HPM6 owns the final release decision. |
| Per-VM minimums and priorities | Contract and durable accounting implemented | Minimums are total-RAM guarantees, all fitting demand grows without priority, and one priority is consulted only under contention. Pure reclaim dependencies require a named waiting higher-priority recipient and a strictly lower-priority shrink-safe donor; coordinator and actuation remain HPM2-HPM4 work. |
| Additional guest-OS providers | Planned | Windows is the first demand provider. HPM5 adds a Linux-native adapter through the common `GuestDemandReport` without sharing raw OS telemetry schemas. |
| Driver status interface | Deferred | Optional diagnostics cannot become allocation authority; implementation requires separate external driver ownership and approval. |

## Platform boundary

The current support target is one fully trusted Windows development guest on a
libvirt/KVM host. Windows virtio-mem remains technology-preview integration;
there is no production, multi-VM, mixed-provider, or untrusted-guest support
claim.

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
