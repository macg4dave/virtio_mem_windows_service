# Roadmap

## Phase 1: Foundation (In Progress)

- [ ] Linux controller scaffolding
- [ ] Windows service scaffolding (conditional; native QGA validation first)
- [ ] QEMU Guest Agent integration research
- [ ] libvirt integration proof-of-concept

## Phase 2: Core Functionality (Planned)

- [ ] Linux controller: Memory metric polling
- [ ] Linux controller: Hysteresis-based allocation logic
- [ ] Windows service: Memory metrics collection
- [ ] Windows service: QEMU Guest Agent exposure
- [ ] End-to-end integration testing

## Phase 3: Hardening (Planned)

- [ ] Error handling and recovery
- [ ] Logging and observability
- [ ] Configuration management
- [ ] Performance tuning
- [ ] Documentation completion

## Phase 4: Operations (Future)

- [ ] Systemd service integration
- [ ] Windows service registration
- [ ] Monitoring and alerting
- [ ] Health checks
- [ ] Metrics export

## Known Risks

- QEMU Guest Agent availability on Windows
- Memory allocation hysteresis tuning
- Performance under memory pressure
- Cross-platform integration complexity
