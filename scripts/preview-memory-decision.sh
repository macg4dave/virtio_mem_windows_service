#!/bin/bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: preview-memory-decision.sh VM_NAME ALIAS

Required environment variables:
  VIRTIO_MEM_MIN_MEMORY_BYTES
  VIRTIO_MEM_MAX_MEMORY_BYTES
  VIRTIO_MEM_LOWER_THRESHOLD_BYTES
  VIRTIO_MEM_UPPER_THRESHOLD_BYTES

Optional:
  --connect URI as the third argument, for example qemu:///system

Exit status:
  0   no resize would be requested
  10  a resize would be requested (nothing was changed)
  20  decision blocked or validation failed
USAGE
}

if [[ "$#" -lt 2 || "$#" -gt 4 ]]; then
  usage
  exit 20
fi

vm_name="$1"
alias="$2"
virsh_args=()
if [[ "${3:-}" == "--connect" ]]; then
  [[ "$#" -eq 4 && -n "$4" ]] || { printf '%s\n' '--connect requires a libvirt URI.' >&2; exit 20; }
  virsh_args=(-c "$4")
elif [[ "$#" -ne 2 ]]; then
  usage
  exit 20
fi
if [[ -z "$vm_name" || ! "$alias" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  printf 'VM_NAME must be non-empty and ALIAS must contain only letters, digits, _, ., or - .\n' >&2
  exit 20
fi

for command_name in virsh jq xmllint; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 20
  fi
done

virsh_command() {
  virsh "${virsh_args[@]}" "$@"
}

positive_env() {
  local name="$1"
  local value="${!name-}"
  if [[ ! "$value" =~ ^[1-9][0-9]*$ ]]; then
    printf '%s must be a positive decimal integer.\n' "$name" >&2
    exit 20
  fi
  printf '%s' "$value"
}

min_memory="$(positive_env VIRTIO_MEM_MIN_MEMORY_BYTES)"
max_memory="$(positive_env VIRTIO_MEM_MAX_MEMORY_BYTES)"
lower_threshold="$(positive_env VIRTIO_MEM_LOWER_THRESHOLD_BYTES)"
upper_threshold="$(positive_env VIRTIO_MEM_UPPER_THRESHOLD_BYTES)"

if (( min_memory > max_memory || lower_threshold > upper_threshold )); then
  printf 'Invalid policy: memory limits or threshold order is invalid.\n' >&2
  exit 20
fi

memory_xpath="/domain/devices/memory[@model='virtio-mem'][alias[@name='$alias']]"
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

as_bytes() {
  local field="$1"
  local value="$2"
  local unit="${3:-B}"
  local factor
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    printf '%s is not a decimal integer: %s\n' "$field" "$value" >&2
    exit 20
  fi
  case "$unit" in
    ''|B|bytes) factor=1 ;;
    KiB) factor=1024 ;;
    MiB) factor=1048576 ;;
    GiB) factor=1073741824 ;;
    *) printf 'Unsupported %s unit: %s\n' "$field" "$unit" >&2; exit 20 ;;
  esac
  if (( value > 9223372036854775807 / factor )); then
    printf '%s overflows host validation arithmetic.\n' "$field" >&2
    exit 20
  fi
  printf '%d' "$((value * factor))"
}

printf 'Read-only memory decision preview: VM=%s alias=%s\n' "$vm_name" "$alias"
printf 'No resize command will be issued by this script.\n'
printf 'VM state: %s\n' "$(virsh_command domstate "$vm_name")"

xml="$(virsh_command dumpxml "$vm_name")"
if [[ "$(xmllint --xpath "count($memory_xpath)" - <<<"$xml")" != "1" ]]; then
  printf 'Expected exactly one virtio-mem device with alias %s.\n' "$alias" >&2
  exit 20
fi

size="$(as_bytes size "$(xml_value "$xml" size)" "$(xml_unit "$xml" size)")"
block="$(as_bytes block "$(xml_value "$xml" block)" "$(xml_unit "$xml" block)")"
requested="$(as_bytes requested "$(xml_value "$xml" requested)" "$(xml_unit "$xml" requested)")"
current="$(as_bytes current "$(xml_value "$xml" current)" "$(xml_unit "$xml" current)")"

printf 'virtio-mem: size=%s block=%s requested=%s current=%s bytes\n' "$size" "$block" "$requested" "$current"

if (( block < 1048576 || (block & (block - 1)) != 0 )); then
  printf 'BLOCKED: block size must be a power of two of at least 1 MiB.\n'
  exit 20
fi
if (( min_memory % block != 0 || max_memory % block != 0 )); then
  printf 'BLOCKED: configured memory limits are not aligned to the live block size.\n'
  exit 20
fi
if (( max_memory > size )); then
  printf 'BLOCKED: configured maximum exceeds the virtio-mem device size.\n'
  exit 20
fi
if (( requested != current )); then
  printf 'BLOCKED: previous request has not converged; service should wait.\n'
  exit 20
fi
if (( current < min_memory || current > max_memory )); then
  printf 'BLOCKED: current memory is outside configured limits.\n'
  exit 20
fi

qga_response="$(virsh_command qemu-agent-command "$vm_name" '{"execute":"guest-get-memory-stats"}')" || {
  printf 'BLOCKED: QEMU Guest Agent memory stats request failed.\n' >&2
  exit 20
}
if ! printf '%s\n' "$qga_response" | jq -e '.return | type == "array"' >/dev/null; then
  printf 'BLOCKED: QGA response does not contain a memory-stat array.\n' >&2
  exit 20
fi
free_memory="$(printf '%s\n' "$qga_response" | jq -er '[.return[] | select(.stat == "stat-free") | .value][0]')" || {
  printf 'BLOCKED: QGA response does not contain stat-free.\n' >&2
  exit 20
}
if [[ ! "$free_memory" =~ ^[0-9]+$ ]]; then
  printf 'BLOCKED: QGA stat-free is not a decimal byte count.\n' >&2
  exit 20
fi
printf 'QGA: free=%s bytes\n' "$free_memory"

if (( free_memory < lower_threshold )); then
  target=$((current + block))
  if (( target > max_memory )); then target="$max_memory"; fi
  if (( target == current )); then
    printf 'NO CHANGE: free memory is below the lower threshold, but already at the maximum.\n'
    exit 0
  fi
  printf 'WOULD REQUEST GROW: target=%s bytes (free below lower threshold).\n' "$target"
  exit 10
fi

if (( free_memory > upper_threshold )); then
  target=$((current - block))
  if (( target < min_memory )); then target="$min_memory"; fi
  if (( target == current )); then
    printf 'NO CHANGE: free memory is above the upper threshold, but already at the minimum.\n'
    exit 0
  fi
  printf 'WOULD REQUEST SHRINK: target=%s bytes (free above upper threshold).\n' "$target"
  exit 10
fi

printf 'NO CHANGE: free memory is inside the hysteresis band.\n'
exit 0
