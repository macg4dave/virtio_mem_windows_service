# Project Status & Next Steps

## Current State: Planning Phase ✅

**Date**: 2026-08-17  
**Phase**: Foundation (Phase 1)  
**Overall Status**: Ready for development

### Completion Summary

- [x] Architecture defined
- [x] Feature matrix created
- [x] API contracts specified
- [x] Data model documented
- [x] Engineering standards defined
- [x] Testing strategy outlined
- [x] Implementation plan created
- [x] QEMU Guest Agent setup guide created
- [x] Go references removed from the repo scope
- [ ] Windows service implementation (planned in Rust)
- [ ] Host automation scripts (planned in Bash)
- [x] Backlog execution board created

## Current Phase 2 work

The Rust library now contains the pure memory-control policy used by the
future service loop. It validates block-aligned limits, applies hysteresis,
waits for requested/current convergence, and clamps each one-block request to
the configured safe range.

## Next task

### TASK-001: Rust service foundation (in progress)

This is the first implementation step. It includes:

1. Rust project structure under `windows/`
2. QEMU Guest Agent response parsing
3. Validation of guest memory metrics
4. Memory-safety and error-handling checks
5. Local test coverage and validation helpers

Completed in this session:

- `parse_memory_stats` handles required fields, optional availability, malformed JSON, and inconsistent values.
- Unit tests cover the expected response, fallback behavior, missing fields, and invalid ranges.

Current blocker:

- Windows-native Rust 1.97.1 MSVC validation now passes; live QGA/libvirt validation remains pending on the KVM host.

Success criteria:

- Rust code compiles without errors
- `cargo test` passes locally
- `cargo clippy` passes locally
- The service is ready for integration with the QEMU Guest Agent

## Technology policy

This repo is intentionally limited to:

- Rust for any service or program logic
- Bash for build and automation scripts

No Go, C#, PowerShell, Python, Java, or other languages are allowed in the source tree.

## Architecture at a glance

```text
Windows 11 guest
  ↕ QEMU Guest Agent
RHEL host
  ↕ Rack of host validation and automation
  ├─ Inspect memory metrics
  ├─ Validate guest-agent behavior
  └─ Verify virtio-mem safety before automation

The runtime implementation is planned in Rust, not Go.
```

## Project stack

### Windows service (Rust)

- Language: Rust 1.70+
- Layer: Windows memory metrics and guest agent integration
- Entry point: `windows/src/main.rs`

### Automation (Bash)

- Build commands and helper scripts
- Keeps the project aligned with the language policy

## Validation checklist

- [ ] `cargo test` in the Windows project
- [ ] `cargo clippy` in the Windows project
- [ ] Bash helper validation passes locally
- [ ] Documentation updated when behavior changes

## References

- [README.md](README.md)
- [BACKLOG.md](BACKLOG.md)
- [docs/architecture.md](docs/architecture.md)
- [docs/feature-matrix.md](docs/feature-matrix.md)
- [docs/testing.md](docs/testing.md)
- [docs/engineering-standards.md](docs/engineering-standards.md)
