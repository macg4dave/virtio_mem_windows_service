# Build and validation tooling

`cargo xtask` is the maintained control plane for repository gates, remote
native-Windows validation, artifact verification, QGA checks, reversible live
resize, and unattended workload qualification. Normal Rust tests remain in
their owning crates.

```bash
cargo xtask help
cargo xtask gate local
cargo xtask deployment inventory INSTANCE REQUIRED_OPTIONS
cargo xtask windows all
cargo xtask windows verify SHA256:EXPECTED_HOST_FINGERPRINT --runs RUN_COUNT
cargo xtask qga VM_NAME --attempts COUNT --command-timeout-seconds SECONDS
cargo xtask live-resize VM_NAME DEVICE_ALIAS TARGET_BYTES REQUIRED_OPTIONS
cargo xtask qualification start VM_NAME DEVICE_ALIAS REQUIRED_OPTIONS
cargo xtask qualification status RUN_ID
cargo xtask qualification review RUN_ID
```

Every endpoint, path, workload size, test duration, retry count, sampling
interval, safety floor, headroom reserve, and timeout used by a remote or live
workflow must be supplied explicitly. `cargo xtask help` is the authoritative
option list. Dry-run remains the default for mutation-capable workflows.

The Windows workflow requires these environment variables:

- `VIRTIO_MEM_WINDOWS_SSH`
- `VIRTIO_MEM_WINDOWS_DIR`
- `VIRTIO_MEM_WINDOWS_ARTIFACTS`
- `VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS`
- `VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS`

Optional pinned-host and identity-file variables are documented by `help` and
`docs/testing.md`. The tool never guesses a guest or remote workspace.

`live-resize` captures initial allocation, requires QGA health, delegates each
forward and rollback request to the attestation-aware host product CLI, waits
for alias-scoped convergence, and restores the captured allocation unless an
explicitly authorized `--keep-target` is supplied.

`qualification start` observes the installed automatic controller; it never
issues a competing resize. It requires an explicit workload binary, workload
sizes and holds, telemetry path, services, timing, and acceptance deltas. With
`--apply`, a detached supervisor writes versioned JSON/JSONL evidence beneath
the selected output root for later `status` and `review` calls.

Editor and Make entrypoints are convenience delegates only. Repeatable logic
belongs here. A privileged workflow validates its exact scope in the
unprivileged process, then re-executes the current prebuilt xtask through one
outer `sudo`; it never generates shell.
