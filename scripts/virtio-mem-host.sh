#!/bin/bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  virtio-mem-host.sh snapshot VM_NAME ALIAS
  virtio-mem-host.sh resize VM_NAME ALIAS TARGET_BYTES

snapshot is read-only. resize is the only mode that issues a live update.
USAGE
}

if [[ "$#" -ne 3 && "$#" -ne 4 ]]; then
  usage
  exit 2
fi

mode="$1"
vm_name="$2"
alias="$3"
target_bytes="${4:-}"

if [[ "$mode" != "snapshot" && "$mode" != "resize" ]]; then
  usage
  exit 2
fi
if [[ -z "$vm_name" || ! "$alias" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  printf 'VM_NAME must be non-empty and ALIAS must contain only letters, digits, _, ., or - .\n' >&2
  exit 2
fi
if [[ "$mode" == "resize" && ! "$target_bytes" =~ ^[1-9][0-9]*$ ]]; then
  printf 'TARGET_BYTES must be a positive decimal integer.\n' >&2
  exit 2
fi

for command_name in virsh xmllint; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  fi
done

xml_snapshot() {
  # virsh dumpxml has no --live option; for a running domain its default
  # output is the live definition. --inactive is the explicit persistent
  # configuration selector and must not be used for resize validation.
  virsh dumpxml "$vm_name"
}

memory_xpath="/domain/devices/memory[@model='virtio-mem'][alias[@name='$alias']]"

selected_memory_count() {
  xmllint --xpath "count($memory_xpath)" - <<<"$1"
}

xml_value() {
  local xml="$1"
  local field="$2"
  xmllint --xpath "string(($memory_xpath//*[local-name()='$field'])[1])" - <<<"$xml"
}

xml_unit() {
  local xml="$1"
  local field="$2"
  xmllint --xpath "string(($memory_xpath//*[local-name()='$field']/@unit)[1])" - <<<"$xml"
}

to_bytes() {
  local field="$1"
  local value="$2"
  local unit="${3:-B}"
  local factor

  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    printf '%s is not a decimal integer: %s\n' "$field" "$value" >&2
    return 1
  fi

  case "$unit" in
    ''|B|bytes) factor=1 ;;
    KiB) factor=1024 ;;
    MiB) factor=1048576 ;;
    GiB) factor=1073741824 ;;
    *) printf 'Unsupported %s unit: %s\n' "$field" "$unit" >&2; return 1 ;;
  esac

  # Bash arithmetic is signed on the supported host shells. Reject values that
  # cannot be represented safely rather than silently wrapping during checks.
  if (( value > 9223372036854775807 / factor )); then
    printf '%s overflows the host validation arithmetic.\n' "$field" >&2
    return 1
  fi
  printf '%d\n' "$((value * factor))"
}

xml="$(xml_snapshot)"
if [[ "$(selected_memory_count "$xml")" != "1" ]]; then
  printf 'Expected exactly one virtio-mem device with alias %s.\n' "$alias" >&2
  exit 1
fi

if [[ "$mode" == "snapshot" ]]; then
  printf '%s\n' "$xml"
  exit 0
fi

size="$(to_bytes size "$(xml_value "$xml" size)" "$(xml_unit "$xml" size)")"
block="$(to_bytes block "$(xml_value "$xml" block)" "$(xml_unit "$xml" block)")"
requested="$(to_bytes requested "$(xml_value "$xml" requested)" "$(xml_unit "$xml" requested)")"
current="$(to_bytes current "$(xml_value "$xml" current)" "$(xml_unit "$xml" current)")"

if (( block < 1048576 )); then
  printf 'Refusing resize: block size is below 1 MiB (%d bytes).\n' "$block" >&2
  exit 1
fi
if (( block & (block - 1) )); then
  printf 'Refusing resize: block size is not a power of two (%d bytes).\n' "$block" >&2
  exit 1
fi
if (( requested != current )); then
  printf 'Refusing resize: requested (%d) has not converged to current (%d).\n' "$requested" "$current" >&2
  exit 1
fi
target="$(to_bytes target "$target_bytes" B)"
if (( target > size )); then
  printf 'Refusing resize: target (%d) exceeds device size (%d).\n' "$target" "$size" >&2
  exit 1
fi
if (( target % block != 0 )); then
  printf 'Refusing resize: target (%d) is not aligned to block size (%d).\n' "$target" "$block" >&2
  exit 1
fi

printf 'Issuing approved live resize: VM=%s alias=%s target=%s bytes\n' "$vm_name" "$alias" "$target_bytes"
virsh update-memory-device "$vm_name" \
  --alias "$alias" \
  --requested-size "$target" \
  --live
