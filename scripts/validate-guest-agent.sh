#!/bin/bash
set -euo pipefail

usage() {
  printf 'Usage: VIRSH_CONNECT=qemu:///system %s VM_NAME [ATTEMPTS]\n' "$0" >&2
}

if [[ "$#" -lt 1 || "$#" -gt 2 ]]; then
  usage
  exit 2
fi

vm_name="$1"
attempts="${2:-3}"
connect="${VIRSH_CONNECT:-qemu:///system}"

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
  virsh -c "$connect" qemu-agent-command "$vm_name" "$request"
}

dommemstat_fallback() {
  local dommemstat_output actual unused
  dommemstat_output="$(virsh -c "$connect" dommemstat "$vm_name")"
  actual="$(awk '$1 == "actual" {print $2}' <<<"$dommemstat_output")"
  unused="$(awk '$1 == "unused" {print $2}' <<<"$dommemstat_output")"
  [[ "$actual" =~ ^[0-9]+$ && "$unused" =~ ^[0-9]+$ ]]
}

printf 'Checking QEMU Guest Agent for VM %s...\n' "$vm_name"
qga_command '{"execute":"guest-info"}' | jq -e '.return' >/dev/null
printf 'guest-info: OK\n'

if stats="$(qga_command '{"execute":"guest-get-memory-stats"}' 2>/dev/null)"; then
  printf '%s\n' "$stats" | jq -e '
    (.return | type == "array") and
    ([.return[] | select(.stat == "stat-free" or .stat == "stat-total") | .value] | length == 2)
  ' >/dev/null
  for ((attempt = 1; attempt <= attempts; attempt++)); do
    qga_command '{"execute":"guest-get-memory-stats"}' | jq -e '
      (.return | type == "array") and
      ([.return[] | select(.stat == "stat-free" or .stat == "stat-total") | .value] | length == 2)
    ' >/dev/null
    printf 'guest-get-memory-stats attempt %d/%d: OK\n' "$attempt" "$attempts"
  done
else
  printf 'guest-get-memory-stats is unavailable; validating dommemstat fallback.\n'
  for ((attempt = 1; attempt <= attempts; attempt++)); do
    dommemstat_fallback
    printf 'dommemstat fallback attempt %d/%d: OK\n' "$attempt" "$attempts"
  done
fi

printf 'QEMU Guest Agent validation passed for %s.\n' "$vm_name"
