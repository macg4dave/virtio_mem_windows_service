#!/bin/bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/windows"

cargo fmt --all -- --check
cargo build --release
cargo test
cargo clippy --all-targets --all-features -- -D warnings
