.PHONY: help local-gate windows-gate win11-build win11-gate win11-status all-gates clean

help:
	@echo "Repository validation delegates"
	@echo "  make local-gate    - cargo xtask gate local"
	@echo "  make windows-gate  - cargo xtask windows all"
	@echo "  make win11-build   - sync, build and fetch the service from win11_gpu"
	@echo "  make win11-gate    - full native Windows gate on win11_gpu"
	@echo "  make win11-status  - compare the installed service with the fetched build"
	@echo "  make all-gates     - cargo xtask gate all"
	@echo "  make clean         - remove Cargo build output"

local-gate:
	cargo xtask gate local

windows-gate:
	cargo xtask windows all

# The established win11_gpu build workspace; environment/command-line values
# override these defaults. SSH identity and host trust remain in ~/.ssh/config.
WIN11_TARGETS := win11-build win11-gate win11-status
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_SSH ?= virtio-mem-windows
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_DIR ?= C:\Users\Dave\github\virtio_mem_windows_service
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_ARTIFACTS ?= .artifacts/windows/win11_gpu
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS ?= 10
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS ?= 1200
WIN11_SERVICE ?= VirtioMemService

win11-build:
	cargo xtask windows check
	cargo xtask windows sync
	cargo xtask windows build
	cargo xtask windows fetch

win11-gate:
	cargo xtask windows all

win11-status:
	cargo xtask windows build-status $(WIN11_SERVICE) --output $(VIRTIO_MEM_WINDOWS_ARTIFACTS)/build-status.json

all-gates:
	cargo xtask gate all

clean:
	cargo clean
