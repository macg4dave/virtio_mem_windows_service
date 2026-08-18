#!/bin/bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo fmt --all -- --check
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
