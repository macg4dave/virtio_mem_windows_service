#!/bin/bash
set -euo pipefail

usage() {
  printf 'Usage: %s VM_NAME [ATTEMPTS]\n' "$0" >&2
}

if [[ "$#" -lt 1 || "$#" -gt 2 ]]; then
  usage
  exit 2
fi

vm_name="$1"
attempts="${2:-3}"

if [[ -z "$vm_name" || ! "$attempts" =~ ^[1-9][0-9]*$ ]]; then
  printf 'VM_NAME must be non-empty and ATTEMPTS must be a positive integer.\n' >&2
  exit 2
fi

for command_name in virsh jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  fi
done

qga_command() {
  local request="$1"
  virsh qemu-agent-command "$vm_name" "$request"
}

printf 'Checking QEMU Guest Agent for VM %s...\n' "$vm_name"
qga_command '{"execute":"guest-info"}' | jq -e '.return' >/dev/null
printf 'guest-info: OK\n'

for ((attempt = 1; attempt <= attempts; attempt++)); do
  stats="$(qga_command '{"execute":"guest-get-memory-stats"}')"
  printf '%s\n' "$stats" | jq -e '
    (.return | type == "array") and
    ([.return[] | select(.stat == "stat-free" or .stat == "stat-total") | .value] | length == 2)
  ' >/dev/null
  printf 'guest-get-memory-stats attempt %d/%d: OK\n' "$attempt" "$attempts"
done

printf 'QEMU Guest Agent validation passed for %s.\n' "$vm_name"
