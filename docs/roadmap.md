# Roadmap

## Phase 1: Foundation (In Progress)

- [~] Rust service scaffolding (parser foundation implemented; service runtime remains)
- [x] QEMU Guest Agent integration research
- [x] libvirt validation proof-of-concept (manual and scripted checks documented)
- [x] Bash validation helpers and local automation

## Phase 2: Core Functionality (In Progress)

- [~] Windows service: Memory metric polling (poller and stoppable interval scheduler implemented; service hosting remains)
- [~] Windows service: QEMU Guest Agent exposure (configurable named-pipe client implemented; live channel validation remains)
- [~] Host-side virtio-mem validation flow (safe resize planning implemented; live XML adapter remains)
- [ ] End-to-end integration testing

## Phase 3: Hardening (Planned)

- [ ] Error handling and recovery
- [ ] Logging and observability
- [~] Configuration management (validated runtime model implemented; persistent loading remains)
- [ ] Performance tuning
- [ ] Documentation completion

## Phase 4: Operations (Future)

- [~] Windows service registration (portable lifecycle host implemented; SCM adapter remains)
- [ ] Host automation automation and checks
- [ ] Monitoring and alerting
- [ ] Health checks
- [ ] Metrics export

## Known Risks

- QEMU Guest Agent availability on Windows
- Memory allocation hysteresis tuning
- Performance under memory pressure
- Cross-platform integration complexity
