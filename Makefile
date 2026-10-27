.PHONY: help local-gate windows-gate win11-build win11-gate win11-status win11-deploy all-gates clean

help:
	@echo "Repository validation delegates"
	@echo "  make local-gate    - cargo xtask gate local"
	@echo "  make windows-gate  - cargo xtask windows all"
	@echo "  make win11-build   - sync, build and fetch the service from win11_gpu"
	@echo "  make win11-gate    - full native Windows gate on win11_gpu"
	@echo "  make win11-status  - compare the installed service with the fetched build"
	@echo "  make win11-deploy  - install the fetched build using the Windows test preset"
	@echo "  make all-gates     - cargo xtask gate all"
	@echo "  make clean         - remove Cargo build output"

local-gate:
	cargo xtask gate local

windows-gate:
	cargo xtask windows all

# Load this machine-specific test preset only for its explicit targets.
WIN11_TARGETS := win11-build win11-gate win11-status win11-deploy
ifneq ($(filter $(WIN11_TARGETS),$(MAKECMDGOALS)),)
include deploy/config_files/windows-build.mk
endif
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_SSH ?= $(WIN11_SSH)
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_DIR ?= $(WIN11_DIR)
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_ARTIFACTS ?= $(WIN11_ARTIFACTS)
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS ?= $(WIN11_CONNECT_TIMEOUT_SECONDS)
$(WIN11_TARGETS): export VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS ?= $(WIN11_OPERATION_TIMEOUT_SECONDS)

win11-build:
	cargo xtask windows check
	cargo xtask windows sync
	cargo xtask windows build
	cargo xtask windows fetch

win11-gate:
	cargo xtask windows all

win11-status:
	cargo xtask windows build-status $(WIN11_SERVICE) --output $(VIRTIO_MEM_WINDOWS_ARTIFACTS)/build-status.json

# Restarts VirtioMemService in the guest with the tracked RAM policy.
win11-deploy:
	cargo xtask windows deploy $(WIN11_MANIFEST) --output $(VIRTIO_MEM_WINDOWS_ARTIFACTS)/deploy.json --apply

all-gates:
	cargo xtask gate all

clean:
	cargo clean
