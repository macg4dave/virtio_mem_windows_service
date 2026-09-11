.PHONY: help local-gate windows-gate all-gates clean

help:
	@echo "Repository validation delegates"
	@echo "  make local-gate    - cargo xtask gate local"
	@echo "  make windows-gate  - cargo xtask windows all"
	@echo "  make all-gates     - cargo xtask gate all"
	@echo "  make clean         - remove Cargo build output"

local-gate:
	cargo xtask gate local

windows-gate:
	cargo xtask windows all

all-gates:
	cargo xtask gate all

clean:
	cargo clean
