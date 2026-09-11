# Windows service

This Rust crate collects native Windows memory telemetry and publishes
versioned allocation-free records. It does not own libvirt, QGA, host policy,
or resize actuation.

## Build and test

Build locally on a configured native Windows toolchain with Cargo. From the
RHEL development checkout, use the authoritative remote gate:

```bash
cargo xtask windows all
```

The endpoint, remote workspace, local artifact directory, connection timeout,
and operation timeout are required environment inputs; see
`../docs/testing.md`. The gate synchronizes source, initializes MSVC, runs the
native build/tests/format/Clippy, fetches the executable, and verifies its
hash. It never installs the service or changes guest memory.

## Configuration and service lifecycle

Production and interactive startup require the versioned JSON configuration at
`C:\ProgramData\VirtioMemService\config.json`. Missing, malformed, unsupported,
or incomplete configuration fails closed. The file explicitly supplies VM and
service identity, display metadata, telemetry path, service account, polling
interval, and shutdown timeout.

The executable supports `install`, `start`, `run`, `stop`, `remove`, and
`help`. Service-manager operations require the privileges of the configured
deployment. Installation provisions the configured ProgramData paths and
least-privilege ACL, registers recovery metadata, and records the configured
identity. The SCM dispatcher rejects an identity mismatch.

Validate candidate hash, SCM configuration, ACLs, telemetry advancement,
events, start/stop behavior, and final state as a separate deployment layer.
Do not combine service lifecycle testing with a live resize.

## Workload qualification binary

`virtio-mem-workload.exe` is a separate test binary. It is never installed as
the service and has no QGA, libvirt, or resize interface. Every allocation
size, safety cap, hold duration, and resident refresh interval is a required
argument. It emits flushed versioned JSON-line phase evidence.

Invoke it through `cargo xtask qualification` for correlated host, telemetry,
controller, and guest-health evidence. Direct invocation alone is not platform
qualification.

See `../docs/testing.md` for supported commands and
`../docs/architecture.md` for component ownership.
