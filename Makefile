.PHONY: build test lint fmt clean help

help:
	@echo "Virtual Memory Controller - Build Targets"
	@echo ""
	@echo "Linux Controller:"
	@echo "  make linux-build      - Build Linux controller"
	@echo "  make linux-test       - Run Go tests"
	@echo "  make linux-fmt        - Format Go code"
	@echo "  make linux-vet        - Vet Go code"
	@echo ""
	@echo "Windows Service:"
	@echo "  make windows-build    - Build Rust service"
	@echo "  make windows-test     - Run Rust tests"
	@echo ""
	@echo "Combined:"
	@echo "  make build            - Build all components"
	@echo "  make test             - Test all components"
	@echo "  make lint            - Lint all components"
	@echo "  make fmt             - Format all components"
	@echo "  make clean           - Clean build artifacts"

linux-build:
	cd linux && go build -o bin/virtio-mem-controller ./cmd/controller

linux-test:
	cd linux && go test ./...

linux-vet:
	cd linux && go vet ./...

linux-fmt:
	cd linux && gofmt -w .

windows-build:
	cd windows && cargo build --release

windows-test:
	cd windows && cargo test

build: linux-build windows-build
	@echo "✓ All components built"

test: linux-test windows-test
	@echo "✓ All tests passed"

lint: linux-vet
	cd windows && cargo clippy --all-targets --all-features -- -D warnings
	@echo "✓ Linting complete"

fmt: linux-fmt
	cd windows && cargo fmt --all
	@echo "✓ Formatting complete"

clean:
	cd linux && rm -rf bin/ pkg/ && go clean ./...
	cd windows && cargo clean
	@echo "✓ Clean complete"
