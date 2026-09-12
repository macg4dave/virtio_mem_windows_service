# virtio-mem Windows service

A safety-first Rust system for publishing Windows guest memory telemetry and
coordinating bounded virtio-mem changes from a RHEL/libvirt host.

## Scope

The current supported development scope is one explicitly configured, fully
trusted Windows guest and one host controller/device. Windows virtio-mem is
technology preview; this repository does not claim production, untrusted-guest,
or multi-VM support.

Windows measures and publishes allocation-free telemetry. The host joins fresh
telemetry with alias-scoped live libvirt `current`, calculates the target,
checks compatibility and headroom, journals intent, and owns actuation.

The project is transitioning from a primarily fixed-headroom calculation to a
Windows-native pressure-aware policy. Windows memory-resource notifications and
supported memory-manager performance data will become the primary demand
evidence; the host will continue to own sizing and actuation. The current fixed
reserve model becomes a fallback/safety guard, not the intended demand
predictor. See `docs/windows-native-pressure-controller.md`.

## Components

- `windows/`: native telemetry, versioned atomic publication, SCM lifecycle,
  configuration, ACLs, and a separate qualification workload binary.
- `host/`: telemetry validation/join, target controller, compatibility
  attestation, libvirt state, actuation, convergence, and durable recovery.
- `crates/virtio-mem-core/`: shared units, XML, policy, reconciliation, and
  evidence contracts.
- `tools/xtask/`: repository gates, remote native-Windows validation, artifact
  verification, QGA readiness, reversible live resize, and unattended
  qualification.

Automatic reclaim is a default-on product capability with an explicit pause
override. The normative target, quanta, history, hysteresis, convergence,
retry, and latch rules live in `docs/target-controller.md`; operational docs do
not copy those values into test profiles.

## Development validation

Run focused Cargo tests in the owning crate, then:

```bash
cargo xtask gate local
```

For native Windows code, configure the remote endpoint, workspace, artifact
directory, connection bound, and operation bound, then run:

```bash
cargo xtask windows all
```

For host/QGA readiness:

```bash
cargo xtask doctor host
cargo xtask qga VM_NAME --attempts COUNT --command-timeout-seconds SECONDS
```

Live resize and unattended qualification require explicit current-run sizes,
paths, identities, timing, safety, and acceptance inputs. Use `cargo xtask help`
and `docs/testing.md`; do not copy values from old evidence.

## Safety boundary

- The Windows service never invokes Linux/libvirt commands or receives resize
  authority.
- The host never treats Windows telemetry as allocation authority.
- Live mutation requires exact target identity, fresh attestation and live
  state, alignment, headroom, configured time bounds, convergence checks, and
  an explicit rollback/final-state contract.
- Repeatable validation, deployment, and privileged-boundary logic belongs in
  Rust. Xtask may re-execute its current prebuilt binary through one reviewed
  outer `sudo`; Cargo never runs as root.
- Reboot, shutdown, deletion of pre-existing resources, persistent platform or
  security changes, disabled safeguards, and non-reversible resize require
  explicit current-turn authorization.

## Current readiness

Core telemetry, single-controller policy/reconciliation, native service
lifecycle, host actuation, and Rust validation tooling are implemented. The
implemented policy is now a compatibility baseline, not the release-candidate
sizing algorithm. The stack remains NO-GO for unattended automatic resizing
until the pressure-aware implementation and its applied qualification,
recovery, and endurance gates in `docs/QA-roadmap.md` pass.

## Documentation

| Document | Purpose |
| --- | --- |
| `BACKLOG.md` | Product execution board and current handoff |
| `docs/roadmap.md` | Windows-native controller milestones and exit gates |
| `docs/architecture.md` | Component ownership and safety boundaries |
| `docs/testing.md` | Current validation and deployment model |
| `docs/target-controller.md` | Normative target and recovery contract |
| `docs/windows-native-pressure-controller.md` | Windows signal audit, research, target architecture, and migration |
| `docs/api-contract.md` | Wire and CLI behavior |
| `docs/data-model.md` | Persistent and in-memory model |
| `docs/feature-matrix.md` | Capability status |
| `docs/build-test-tooling-roadmap.md` | Tooling execution board |
| `docs/QA-roadmap.md` | Unattended single-controller qualification gates |
| `docs/dependencies.md` | Current dependency requirements |
