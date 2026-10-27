#!/usr/bin/env bash
# Install deploy/config_files/host_list.toml and (re)start the win11_gpu controller.
# Prompts for sudo. Stopping the controller does not change the device.
set -euo pipefail
cd "$(dirname "$0")/../.."
unit=virtio-mem-host@win11_gpu.service
# Generated per VM definition by `cargo xtask attestation`; not tracked.
attestation=${ATTESTATION:-.artifacts/install/win11_gpu.attestation.json}
if pgrep -f 'target/release/virtio-mem-host' >/dev/null; then
  echo "a foreground virtio-mem-host is running; stop it first" >&2
  exit 1
fi
mkdir -p .artifacts/deploy
cargo build --release --locked -p virtio-mem-host
# host-deploy requires the unit inactive.
sudo systemctl stop "$unit"
cargo xtask host-deploy win11_gpu \
  --config deploy/config_files/host_list.toml \
  --attestation "win11_gpu=$attestation" \
  --output ".artifacts/deploy/host-deploy-$(date +%Y%m%dT%H%M%S).json" \
  --command-timeout-seconds 60 --apply --elevate
sudo systemctl enable --now "$unit"
sleep 5
systemctl status "$unit" --no-pager | head -12
