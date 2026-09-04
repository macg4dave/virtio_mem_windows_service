.PHONY: build test lint fmt clean help windows-build windows-test host-build host-test windows-native all-gates

help:
	@echo "Virtual Memory Controller - Build Targets"
	@echo ""
	@echo "Windows Service:"
	@echo "  make windows-build    - Build Rust service"
	@echo "  make windows-test     - Run Rust tests"
	@echo "  make host-build       - Build RHEL host controller"
	@echo "  make host-test        - Run RHEL host-controller tests"
	@echo ""
	@echo "Automation:"
	@echo "  make build            - Build RHEL-compatible crates"
	@echo "  make test             - Test RHEL-compatible crates"
	@echo "  make lint             - Lint RHEL-compatible crates"
	@echo "  make windows-native   - Run the native Windows gate over SSH"
	@echo "  make all-gates        - Run RHEL and native Windows gates"
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
	cargo build -p virtio-mem-core -p virtio-mem-host --all-features --release --locked
	@echo "✓ RHEL-compatible Rust crates built"

test:
	cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked
	@echo "✓ RHEL-compatible Rust crate tests passed"

lint:
	cargo clippy -p virtio-mem-core -p virtio-mem-host --all-targets --all-features --locked -- -D warnings
	@echo "✓ Linting complete"

windows-native:
	bash scripts/windows-remote-build.sh all

all-gates:
	bash scripts/build-rust.sh
	bash scripts/windows-remote-build.sh all

fmt:
	cargo fmt --all
	@echo "✓ Formatting complete"

clean:
	cargo clean
	@echo "✓ Workspace clean complete"
