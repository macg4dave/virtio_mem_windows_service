# Implementation Plan

## Phase 1: Foundation (CURRENT)

This phase establishes the initial repository structure and validates the QEMU Guest Agent integration path.

### TASK-001: Rust service foundation

**Owner**: Copilot  
**Status**: Planned  
**Effort**: 4-6 hours

#### Deliverables

1. **Directory Structure**
   ```text
   windows/
   ├── src/
   │   └── main.rs
   ├── Cargo.toml
   ├── Cargo.lock
   └── README.md
   ```

2. **Core Components**
   - [ ] Guest-side memory collection entry point
   - [x] QEMU Guest Agent parsing and validation foundation
   - [x] Error handling and safe defaults for memory-stat responses
   - [x] Local unit tests for parsing and invalid values
   - [ ] Bash helper scripts for validation and local checks

3. **Implementation Constraints**
   - Use Rust for the runtime logic
   - Use Bash for local automation and validation steps

4. **Validation**
   - [ ] `cargo build --release`
   - [ ] `cargo test`
   - [ ] `cargo clippy --all-targets --all-features -- -D warnings`
   - [ ] `cargo fmt --all`

#### Acceptance Criteria

- The Rust service compiles without errors
- Unit tests cover parsing and validation logic
- Local lint and build checks pass
- The implementation remains compatible with the repo’s Rust/Bash-only rule

#### Blockers / Dependencies

- QEMU Guest Agent must be running on Windows for live validation
- Host-side validation requires libvirt and `virsh` access

---

### TASK-002: QEMU Guest Agent validation

**Owner**: Unassigned  
**Status**: Ready  
**Effort**: 2-3 hours

#### Deliverables

1. **Validation Checklist**
   - [ ] QEMU Guest Agent service exists on Windows 11
   - [ ] Guest Agent channel is configured in libvirt domain XML
   - [ ] The host can execute `virsh qemu-agent-command`
   - [ ] Guest Agent responds to `guest-info`
   - [ ] Guest Agent responds to `guest-get-memory-stats`
   - [ ] Unix socket communication works reliably

2. **Documentation**
   - [ ] Update [docs/qemu-ga-setup.md](docs/qemu-ga-setup.md) with step-by-step setup
   - [ ] Document expected response formats for memory stats
   - [ ] Record error conditions and recovery strategies

3. **Proof of Concept**
   - [ ] Manual command-line test of each API call
   - [ ] Capture actual JSON responses from Guest Agent
   - [ ] Measure response latency and consistency
   - [ ] Test socket recovery after temporary disconnection

#### Acceptance Criteria

- QEMU Guest Agent is confirmed operational
- Required API endpoints are documented with examples
- At least 3 successful round-trips are observed per endpoint
- Known failure modes are documented
- Setup instructions are reproducible

#### Blockers / Dependencies

- Requires running QEMU guest with Windows 11
- Requires libvirt host access
- QEMU version must support the guest agent features in use

---

### TASK-003: Bash validation helpers

**Owner**: Unassigned  
**Status**: Planned  
**Effort**: 1-2 hours

#### Deliverables

- [ ] Local validation script for environment checks
- [ ] Bash helper for cargo build and test invocation
- [ ] Explicit error handling with `set -euo pipefail`
- [ ] Documentation of expected host prerequisites

#### Acceptance Criteria

- The helper scripts run locally without silent failures
- Required tool checks fail clearly when prerequisites are missing
- Bash is used only for automation, not runtime logic

---

## Phase 2: Core Functionality (Planned)

- **TASK-004**: Windows service memory polling logic
- **TASK-005**: Safe QEMU Guest Agent response handling
- **TASK-006**: Host-side virtio-mem validation flow
- **TASK-007**: End-to-end integration testing

## Phase 3: Hardening (Planned)

- **TASK-008**: Error handling and recovery
- **TASK-009**: Logging and observability
- **TASK-010**: Configuration externalization
- **TASK-011**: Performance tuning

## Phase 4: Operations (Future)

- **TASK-012**: Windows service registration
- **TASK-013**: Host automation and checks
- **TASK-014**: Monitoring and alerting
- **TASK-015**: Health checks

---

## Task Dependencies

```text
TASK-001 (Rust scaffolding)
    ↓
TASK-002 (QEMU GA validation)
    ↓
TASK-003 (Bash validation helpers)
    ↓
TASK-004 (Memory polling logic)
TASK-005 (QGA response handling)
    ↓
TASK-006 (Virtio-mem validation)
    ↓
TASK-007 (Integration testing)
    ↓
Phase 2/3/4 tasks
```

## Execution Rules

1. Complete one task at a time in the **Ready Queue**
2. Move completed tasks to **Completed** in BACKLOG.md
3. Update documentation with every completed task
4. Perform local Rust and Bash validation before marking complete
5. Document any blockers or handoff notes

---

## Quality Gates

### Gate 1: Phase 1 Completion
- [ ] Rust service compiles and all tests pass
- [ ] QEMU Guest Agent integration is validated
- [ ] Bash helper scripts run cleanly
- [ ] All documentation is current

### Gate 2: Phase 2 Completion
- [ ] End-to-end guest validation succeeds on a test VM
- [ ] Memory-read logic is stable across repeated checks
- [ ] No silent failures during validation loops
- [ ] Integration checks are documented

### Gate 3: Phase 3 Completion
- [ ] All error conditions are handled gracefully
- [ ] Logs are readable and actionable
- [ ] Configuration is externalized when needed
- [ ] Performance meets targets

### Gate 4: Phase 4 Completion
- [ ] Service registration is documented and validated
- [ ] Automation checks are repeatable
- [ ] Health checks pass continuously
- [ ] Production deployment is documented
