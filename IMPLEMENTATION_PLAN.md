# Implementation Plan

## Phase 1: Foundation (CURRENT)

This phase establishes the core project scaffolding and validates the QEMU Guest Agent integration.

### TASK-001: Linux Controller Foundation

**Owner**: Copilot  
**Status**: In Progress  
**Effort**: 4-6 hours

#### Deliverables

1. **Directory Structure**
   ```
   linux/
   ├── cmd/
   │   └── controller/
   │       └── main.go
   ├── pkg/
   │   ├── qemu/
   │   │   └── agent.go           # QEMU Guest Agent client
   │   ├── libvirt/
   │   │   └── client.go          # libvirt interface
   │   └── metrics/
   │       └── calculator.go      # Hysteresis logic
   ├── internal/
   │   └── config/
   │       └── config.go          # Config parsing
   ├── go.mod
   ├── go.sum
   ├── Makefile
   └── README.md
   ```

2. **Core Components**
   - [ ] CLI entry point with graceful shutdown
   - [x] QEMU Agent client (host `virsh qemu-agent-command` adapter)
   - [x] Libvirt wrapper (live XML inspection and `virsh update-memory-device`)
   - [x] Metrics calculator (hysteresis logic)
   - [x] Configuration loader (safe defaults)
   - [ ] Logging setup (structured, with verbosity levels)

3. **Go Dependencies**
   - `github.com/libvirt/libvirt-go` (already in go.mod)
   - Consider: `github.com/sirupsen/logrus` for logging
   - Consider: `github.com/spf13/cobra` for CLI if complexity grows

4. **Validation**
   - [ ] `go mod tidy`
   - [ ] `go fmt ./...`
   - [x] `go vet ./...`
   - [x] `go test ./...` (unit tests with mocks)
   - [x] Build produces binary: `./virtio-mem-controller`

#### Acceptance Criteria

- Controller builds without errors
- All packages have unit tests (mocked QEMU/libvirt)
- Code passes `go vet` and `gofmt`
- Can read configuration defaults
- Logs structure is consistent

#### Blockers / Dependencies

- Requires libvirt development headers (`libvirt-devel` on RHEL)
- QEMU Guest Agent must be running on Windows (test in TASK-003)

---

### TASK-002: Windows Service (Deferred)

**Owner**: Unassigned  
**Status**: Deferred pending native QGA validation  
**Effort**: Re-estimate after a demonstrated capability gap

#### Deliverables

1. **Prerequisite decision**
   - Validate native `guest-get-memory-stats` and host-side QGA access first.
   - Define a concrete missing metric or guest lifecycle requirement before adding service code.
   - If required, design the Rust service contract and Windows service lifecycle as a separate task.

2. **Validation gate**
   - [ ] Native QGA is shown insufficient by a documented real-VM test.
   - [ ] Rust service responsibility and protocol are documented before implementation.

#### Acceptance Criteria

- A concrete native-QGA gap is documented.
- Rust service responsibility and API contract are reviewed.
- Windows service lifecycle and metrics behavior have unit tests.

#### Blockers / Dependencies

- Requires an observed native-QGA limitation.
- Windows 11 for testing.
- QEMU Guest Agent must be available.

---

### TASK-003: QEMU Guest Agent Integration Research

**Owner**: Unassigned  
**Status**: Ready  
**Effort**: 2-3 hours

#### Deliverables

1. **Validation Checklist**
   - [ ] QEMU Guest Agent service exists on Windows 11
   - [ ] Guest Agent channel is configured in libvirt domain XML
   - [ ] Can execute `virsh qemu-agent-command` from RHEL host
   - [ ] Guest Agent responds to `guest-info` command
   - [ ] Guest Agent responds to `guest-get-memory-stats`
   - [ ] Unix socket communication works reliably

2. **Documentation**
   - [ ] Update [docs/qemu-ga-setup.md](docs/qemu-ga-setup.md) with step-by-step setup
   - [ ] Document expected response formats for memory stats
   - [ ] Document error codes and recovery strategies
   - [ ] Record any version-specific quirks or limitations

3. **Proof of Concept**
   - [ ] Manual command-line test of each API call
   - [ ] Capture actual JSON responses from Guest Agent
   - [ ] Test memory query latency and consistency
   - [ ] Test socket recovery after disconnection

#### Acceptance Criteria

- QEMU Guest Agent is confirmed operational
- All required API endpoints are documented with examples
- At least 3 successful round-trips for each endpoint
- Known failure modes are documented
- Setup instructions are reproducible

#### Blockers / Dependencies

- Requires running QEMU guest with Windows 11
- Requires libvirt host access
- QEMU version must support virtio-mem (verified separately)

---

## Phase 2: Core Functionality (Planned)

- **TASK-004**: Linux controller memory polling loop
- **TASK-005**: Hysteresis-based allocation logic
- **TASK-006**: Windows service metrics exposure via pipe
- **TASK-007**: End-to-end integration testing

## Phase 3: Hardening (Planned)

- **TASK-008**: Error handling and recovery
- **TASK-009**: Logging and observability
- **TASK-010**: Configuration externalization
- **TASK-011**: Performance tuning

## Phase 4: Operations (Future)

- **TASK-012**: systemd integration (Linux)
- **TASK-013**: Windows service registration (Windows)
- **TASK-014**: Monitoring and alerting
- **TASK-015**: Health checks

---

## Task Dependencies

```
TASK-001 (Linux scaffolding)
    ↓
TASK-002 (Windows scaffolding)
    ↓
TASK-003 (QEMU GA validation)
    ↓
TASK-004 (Polling loop)
TASK-005 (Hysteresis logic)
    ↓
TASK-006 (Metrics exposure)
    ↓
TASK-007 (Integration testing)
    ↓
Phase 2/3/4 tasks
```

## Execution Rules

1. Complete one task at a time in the **Ready Queue**
2. Move completed tasks to **Completed** section in BACKLOG.md
3. Update documentation with every completed task
4. Run local validation (build, test, vet, fmt) before marking complete
5. Document any blockers or handoff notes

---

## Quality Gates

### Gate 1: Phase 1 Completion
- [ ] Linux controller builds and all tests pass
- [ ] Windows service builds and all tests pass
- [ ] QEMU Guest Agent integration is validated
- [ ] All documentation is current

### Gate 2: Phase 2 Completion
- [ ] End-to-end test: Windows metrics flow to Linux controller
- [ ] Memory allocation changes are observed in both directions
- [ ] Poll cycle completes without errors (10 cycles minimum)
- [ ] No memory leaks or resource exhaustion

### Gate 3: Phase 3 Completion
- [ ] All error conditions are handled gracefully
- [ ] Logs are readable and actionable
- [ ] Configuration is externalized
- [ ] Performance meets targets (sub-second response, <5% CPU)

### Gate 4: Phase 4 Completion
- [ ] Services are registered and auto-start correctly
- [ ] Metrics are exported and readable
- [ ] Health checks pass continuously
- [ ] Production deployment is documented
