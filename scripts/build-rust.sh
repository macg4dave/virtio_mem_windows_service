#!/bin/bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo fmt --all -- --check
cargo build -p virtio-mem-core -p virtio-mem-host --all-features --release --locked
cargo test -p virtio-mem-core -p virtio-mem-host --all-features --locked
cargo clippy -p virtio-mem-core -p virtio-mem-host --all-targets --all-features --locked -- -D warnings
bash -n scripts/*.sh
