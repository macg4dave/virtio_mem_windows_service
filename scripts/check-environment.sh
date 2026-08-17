#!/bin/bash
set -euo pipefail

required_commands=(virsh jq)
missing=0

for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  printf 'Install the missing host tools before running guest-agent validation.\n' >&2
  exit 1
fi

printf 'Host validation prerequisites are available.\n'
