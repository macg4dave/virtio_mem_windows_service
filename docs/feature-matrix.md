# Feature Matrix

| Capability | Status | Authority and remaining work |
| --- | --- | --- |
| Windows native memory measurement | Implemented | Windows measures physical and commit state; it has no resize authority. Current deployment evidence remains. |
| Atomic raw telemetry delivery | Implemented | Identity, provenance, ordering, retention, size bounds, and replay acknowledgement are enforced. Installed ACL/publication evidence remains. |
| Windows SCM lifecycle | Implemented | Required configuration, identity propagation, cancellation, EventData, and explicit failures are covered; packaged classic message text remains open. |
| QGA integration | Implemented health boundary | Maintained use is guest health and identity. The custom memory command is experimental and not a production dependency. |
| Live virtio-mem state | Implemented | The host selects one explicit alias; live libvirt `current` is allocation authority. |
| Compatibility and headroom gates | Implemented | Fresh attestation, device bounds, host reserve, freshness, convergence, and ownership fail closed before mutation. |
| Quantitative desired target | Implemented | Physical/commit candidates, safe floor, history, hysteresis, and capacity limitation use validated configurable policy. |
| Desired/requested/current reconciliation | Implemented | Intent journaling, partial progress, upward supersession, uncertain results, restart recovery, and latches prohibit blind replay. |
| Rust build/test tooling | Implemented | `cargo xtask` is authoritative for maintained local, Windows, host, live-resize, and qualification workflows. |
| Single-VM live qualification | In progress | A coherent deployment and complete applied evidence set are required. |
| Multi-VM global pool | Prototype only | Side-effect-free arbitration exists in the shared core; a durable host-wide coordinator, atomic reservation ownership, runtime integration, and live qualification remain AR5–AR6 work. |
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
