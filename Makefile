.PHONY: build test lint fmt clean help

help:
	@echo "Virtual Memory Controller - Build Targets"
	@echo ""
	@echo "Windows Service:"
	@echo "  make windows-build    - Build Rust service"
	@echo "  make windows-test     - Run Rust tests"
	@echo ""
	@echo "Automation:"
	@echo "  make lint             - Lint the Rust component"
	@echo "  make fmt              - Format Rust code"
	@echo "  make clean            - Clean Rust build artifacts"

windows-build:
	cd windows && cargo build --release

windows-test:
	cd windows && cargo test

build: windows-build
	@echo "✓ Rust component built"

test: windows-test
	@echo "✓ Rust tests passed"

lint:
	cd windows && cargo clippy --all-targets --all-features -- -D warnings
	@echo "✓ Linting complete"

fmt:
	cd windows && cargo fmt --all
	@echo "✓ Formatting complete"

clean:
	cd windows && cargo clean
	@echo "✓ Clean complete"
