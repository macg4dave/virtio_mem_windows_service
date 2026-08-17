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
- [x] Linux controller foundation implemented (policy, QGA adapter, libvirt/virsh adapter, tests)
- [ ] Windows service implementation (deferred pending native QGA validation)
- [x] Backlog execution board created

## Next task

### TASK-001: Linux controller foundation

This is the first implementation step. It includes:

1. Go project structure under `linux/`
2. QEMU Guest Agent client
3. libvirt integration wrapper
4. Memory calculation logic for hysteresis
5. Logging and unit tests

Success criteria:

- Go code compiles without errors
- `go vet ./...` passes
- `go test ./...` passes
- The controller is ready for integration with the QEMU Guest Agent

## Technology policy

This repo is intentionally limited to:

- Go for the Linux controller
- Rust for the Windows service
- Bash for build and automation scripts

No C#, PowerShell, Python, Java, or other languages are allowed in the source tree.

## Architecture at a glance

```text
Windows 11 guest
  ↕ QEMU Guest Agent
RHEL host
  ↕ Go controller
  ├─ Query memory stats
  ├─ Apply hysteresis logic
  └─ Call libvirt to adjust virtio-mem

Windows service (Rust) is deferred. Native QEMU Guest Agent statistics are the first implementation path.
```

## Project stack

### Linux controller (Go)

- Language: Go 1.20+
- Layer: libvirt/QEMU interaction and polling
- Entry point: `linux/cmd/controller`

### Windows service (Rust)

- Language: Rust 1.70+
- Layer: Windows memory metrics and guest agent integration
- Entry point: `windows/src/main.rs`

### Automation (Bash)

- Build commands and helper scripts
- Keeps the project aligned with the language policy

## Validation checklist

- [x] `go test ./...` in the Linux project
- [x] `go vet ./...` in the Linux project
- [ ] `cargo test` in the Windows project
- [ ] `cargo clippy` in the Windows project
- [ ] Documentation updated when behavior changes

## References

- [README.md](README.md)
- [BACKLOG.md](BACKLOG.md)
- [docs/architecture.md](docs/architecture.md)
- [docs/feature-matrix.md](docs/feature-matrix.md)
- [docs/testing.md](docs/testing.md)
- [docs/engineering-standards.md](docs/engineering-standards.md)
