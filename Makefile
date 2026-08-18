.PHONY: build test lint fmt clean help

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
	@echo "  make lint             - Lint the Rust component"
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
	cargo build --workspace --release
	@echo "✓ Rust workspace built"

test:
	cargo test --workspace
	@echo "✓ Rust workspace tests passed"

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	@echo "✓ Linting complete"

fmt:
	cargo fmt --all
	@echo "✓ Formatting complete"

clean:
	cargo clean
	@echo "✓ Workspace clean complete"
