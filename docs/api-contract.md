# API and Operational Contract

## Authority

Windows measures and publishes telemetry. It does not read host allocation,
run libvirt commands, or choose a resize. The Linux host selects one explicit
VM/device alias, joins telemetry with live libvirt state, calculates targets,
and owns every mutation.

Alias-scoped live libvirt `current` is allocation authority. QGA and
`dommemstat` are health/demand inputs only.

## Windows service

The executable supports interactive execution and SCM lifecycle operations.
SCM identity must match the configured identity. Configuration is required at
the configured path; missing or invalid configuration fails visibly.

The service emits bounded structured lifecycle and failure EventData. Stop is
cooperative and bounded by the configured shutdown duration. Native telemetry
publication uses an atomic current-record handoff with protected deployment
permissions. Windows retries only transient access-denied replacement
collisions for a bounded interval so a standard QGA file reader cannot
terminate publication; other errors remain visible and fatal.

The optional custom QGA memory adapter is an experimental API boundary. Its
operation deadline must be provided by its caller. Production telemetry does
not depend on that command.

## Host service

The host controller loads validated environment configuration and runs one
controller/device. It requires fresh telemetry, live device state, compatibility
attestation, host headroom, convergence or a qualified upward supersession,
and unlatched command ownership before mutation.

Host instance files follow systemd `EnvironmentFile=` parsing. Windows paths
containing backslashes are single-quoted; typed deployment rejects unquoted
backslashes so the installed value cannot silently differ from review input.

Automatic shrink is enabled by default. An explicit pause remains available.
Same-target diagnostic re-notification is independently disabled by default.
Any ambiguity, stale input, incompatible state, ownership conflict, or unsafe
capacity condition fails closed.

Production raw telemetry uses the explicitly selected `qga-file` transport.
The host opens the configured protected Windows current-record path with the
standard QGA file API, performs a byte- and chunk-bounded read through EOF,
rejects mismatched counts and zero progress, and closes the handle on success
or failure. The existing host
identity, freshness, session, replay, and durable acknowledgement checks apply
after transport decoding. The `file` transport remains available only for an
explicitly provisioned host-local handoff.

The service manager does not blindly restart a failed controller. Recovery and
resumption are explicit operator decisions made after current live state and
durable command state are reviewed.

## Host CLI

The Rust host binary is the implementation behind maintained operations:

- compatibility-attestation generation;
- live snapshot and validation;
- dry-run-default one-shot resize;
- abandon-to-current recovery;
- controller runtime.

Commands require explicit target identity and command timeout. Mutation requires
an explicit apply flag. Recovery additionally requires explicit sampling and
convergence durations.

## Xtask workflows

`cargo xtask` is the repository workflow API:

- `gate local|all`;
- `doctor host`;
- `qga`;
- `windows build|test|all|verify|deploy|service-cycle|diagnose-service`;
- `deployment inventory`, `host-deploy`, `calibration`, `attestation`, and
  restart-safe no-actuation `preflight`;
- `live-resize`;
- `qualification start|status|review`.

Cross-machine paths and deadlines are explicit configuration. Live resize is
dry-run by default and delegates mutation to the product host CLI, so there is
one safety implementation. Qualification uses one bounded elevated guard to
start and stop the disabled controller, while its unprivileged observer reads
production QGA telemetry and never creates a competing resize authority.

Current syntax is emitted by `cargo xtask help`; documentation must not copy a
historical invocation as a default profile.

## Persistence contracts

Telemetry acknowledgement, target-policy state, and command intent are
versioned, bounded, identity-bound, durably flushed, and atomically replaced.
Malformed control state fails closed. A new producer session restarts reclaim
warm-up. Process restart never grants permission to replay a recorded command.

## Compatibility contract

Attestation covers the selected target, allocation-neutral domain and QEMU
inputs, required QOM properties, relevant versions, driver review, workload
exclusions, and capacity review. Every resize recollects and compares evidence.
Allocation progress alone does not invalidate the document; identity,
topology, property, version, trust, or workload drift does.

## Change rule

A schema, command, environment variable, authority boundary, persistence
behavior, or default-policy change requires focused tests and synchronized
updates to this document, the data model, testing guide, and relevant tracker.
Run-specific values do not belong in this contract.
