# Build and test tooling

`tools/xtask` is the repository's authoritative repository-gate,
remote-Windows, and higher-level workflow control plane. It does not replace
normal Rust unit, integration, regression, or doctests; add and run those in
the crate that owns the changed behavior first. Run xtask from the repository
root through the Cargo alias:

```bash
cargo xtask help
cargo xtask gate local
cargo xtask windows all
cargo xtask qga VM_NAME --attempts 3
cargo xtask live-resize VM_NAME ALIAS TARGET_BYTES
cargo xtask qualification start VM_NAME ALIAS --ssh-target WINDOWS_SSH --profile m10g-resident
cargo xtask qualification status RUN_ID
cargo xtask qualification review RUN_ID
```

The tool is intentionally a workspace member. Its parsing and safety checks
are unit tested with the rest of the RHEL-compatible crates, and it reuses the
shared Rust virtio-mem XML and unit contracts rather than maintaining copies in
shell.

`cargo xtask gate local` validates the shared core, host controller, and this
tool on RHEL after focused code-level testing. `cargo xtask gate all` adds the
explicitly configured native Windows gate. It does not treat a Linux build as
Windows evidence or a repository gate as a substitute for targeted tests.

Live mutation remains opt-in. `live-resize` is a dry run unless `--apply` is
present, rejects unsafe or divergent state, bounds forward convergence to 30
seconds, and restores the captured allocation unless `--keep-target` is
explicitly supplied. `--keep-target` is non-reversible and requires explicit
approval under the repository safety rules.

`qualification start` owns unattended, multi-minute Windows workload and
automatic-controller observation runs. It is also dry-run by default. With
`--apply`, it creates a unique directory under
`.vscode-artifacts/qualification/`, launches a detached Rust supervisor, and
returns the run ID immediately. The supervisor retains the SSH workload
session, samples host and guest memory plus alias-scoped requested/current
state, records observed resize requests, and archives the controller journal.
Use `status` while it runs and `review` after it finishes. The versioned JSON
and JSON-lines files are the durable interface for later human or AI analysis.

The qualification harness observes the installed automatic controller; it
does not issue competing resize commands. The Windows workload releases every
mapping on completion, after which the supervisor observes a bounded reclaim
window. It records, but does not force, the final device allocation.

See [`../docs/build-test-tooling-roadmap.md`](../docs/build-test-tooling-roadmap.md)
for the migration inventory, task namespace, dependencies, and remaining
consolidation work. See [`../docs/testing.md`](../docs/testing.md) for the test
selection model, workflow organization, and reporting contract.
