.PHONY: build test lint fmt clean help windows-build windows-test host-build host-test windows-native all-gates

help:
	@echo "Virtual Memory Controller - Build Targets"
	@echo ""
	@echo "Windows Service:"
	@echo "  make windows-build    - Build Rust service on this Windows host"
	@echo "  make windows-test     - Test Rust service on this Windows host"
	@echo "  make host-build       - Build RHEL host controller"
	@echo "  make host-test        - Run RHEL host-controller tests"
	@echo ""
	@echo "Automation:"
	@echo "  make build            - Compatibility alias for cargo xtask gate build"
	@echo "  make test             - Compatibility alias for cargo xtask gate test"
	@echo "  make lint             - Compatibility alias for cargo xtask gate lint"
	@echo "  make windows-native   - Compatibility alias for cargo xtask windows all"
	@echo "  make all-gates        - Compatibility alias for cargo xtask gate all"
	@echo "  make fmt              - Format Rust code"
	@echo "  make clean            - Clean Rust build artifacts"

windows-build:
	cd windows && cargo build --release

windows-test:
	cd windows && cargo test

host-build:
	cargo build -p virtio-mem-host --release

host-test:
	cargo test -p virtio-mem-host

build:
	cargo xtask gate build

test:
	cargo xtask gate test

lint:
	cargo xtask gate lint

windows-native:
	cargo xtask windows all

all-gates:
	cargo xtask gate all

fmt:
	cargo fmt --all
	@echo "✓ Formatting complete"

clean:
	cargo clean
	@echo "✓ Workspace clean complete"
