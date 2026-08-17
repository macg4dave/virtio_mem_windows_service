# Roadmap

## Phase 1: Foundation (In Progress)

- [ ] Rust service scaffolding
- [ ] QEMU Guest Agent integration research
- [ ] libvirt validation proof-of-concept
- [ ] Bash validation helpers and local automation

## Phase 2: Core Functionality (Planned)

- [ ] Windows service: Memory metric polling
- [ ] Windows service: QEMU Guest Agent exposure
- [ ] Host-side virtio-mem validation flow
- [ ] End-to-end integration testing

## Phase 3: Hardening (Planned)

- [ ] Error handling and recovery
- [ ] Logging and observability
- [ ] Configuration management
- [ ] Performance tuning
- [ ] Documentation completion

## Phase 4: Operations (Future)

- [ ] Windows service registration
- [ ] Host automation automation and checks
- [ ] Monitoring and alerting
- [ ] Health checks
- [ ] Metrics export

## Known Risks

- QEMU Guest Agent availability on Windows
- Memory allocation hysteresis tuning
- Performance under memory pressure
- Cross-platform integration complexity
