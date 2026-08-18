#!/bin/bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage:
  live-resize-test.sh VM_NAME ALIAS TARGET_BYTES [OPTIONS]

Options:
  --apply                 Issue the explicitly requested live resize.
  --keep-target           Do not roll back after convergence (requires --apply).
  --timeout SECONDS       Convergence timeout; default: 300.
  --interval SECONDS      Sample interval; default: 5.
  --connect URI           Libvirt URI, for example qemu:///system.
  --log PATH              Append CSV samples to PATH.
  -h, --help              Show this help.

Without --apply, the script only validates the target and prints the baseline.
With --apply, the test waits for convergence and rolls back to the original
requested size unless --keep-target is also supplied.

This script never changes a VM unless --apply is present.
USAGE
}

if [[ "$#" -lt 3 ]]; then
  usage
  exit 2
fi

vm_name="$1"
alias="$2"
target_bytes="$3"
shift 3
apply=0
keep_target=0
timeout_seconds=300
interval_seconds=5
log_path=""
virsh_args=()

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --apply) apply=1; shift ;;
    --keep-target) keep_target=1; shift ;;
    --timeout)
      [[ "$#" -ge 2 && "$2" =~ ^[1-9][0-9]*$ ]] || { printf '%s\n' '--timeout requires a positive integer.' >&2; exit 2; }
      timeout_seconds="$2"
      shift 2
      ;;
    --interval)
      [[ "$#" -ge 2 && "$2" =~ ^[1-9][0-9]*$ ]] || { printf '%s\n' '--interval requires a positive integer.' >&2; exit 2; }
      interval_seconds="$2"
      shift 2
      ;;
    --connect)
      [[ "$#" -ge 2 && -n "$2" ]] || { printf '%s\n' '--connect requires a libvirt URI.' >&2; exit 2; }
      virsh_args=(-c "$2")
      shift 2
      ;;
    --log)
      [[ "$#" -ge 2 && -n "$2" ]] || { printf '%s\n' '--log requires a path.' >&2; exit 2; }
      log_path="$2"
      shift 2
      ;;
    -h|--help) usage >&2; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; usage; exit 2 ;;
  esac
done

virsh_command() {
  virsh "${virsh_args[@]}" "$@"
}

if [[ -z "$vm_name" || ! "$alias" =~ ^[A-Za-z0-9_.-]+$ ]]; then
  printf 'VM_NAME must be non-empty and ALIAS must contain only letters, digits, _, ., or - .\n' >&2
  exit 2
fi
if [[ ! "$target_bytes" =~ ^[1-9][0-9]*$ ]]; then
  printf 'TARGET_BYTES must be a positive decimal integer.\n' >&2
  exit 2
fi
if (( keep_target == 1 && apply == 0 )); then
  printf '%s\n' '--keep-target requires --apply.' >&2
  exit 2
fi

for command_name in virsh xmllint jq; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 20
  fi
done

if [[ -n "$log_path" ]]; then
  printf 'timestamp,domstate,requested_bytes,current_bytes,size_bytes,block_bytes,qga_free_bytes,qga_total_bytes\n' >>"$log_path"
fi

log() {
  printf '%s\n' "$*"
}
record_sample() {
  local xml state size block requested current qga_response qga_free qga_total timestamp
  xml="$(virsh_command dumpxml "$vm_name")"
  state="$(virsh_command domstate "$vm_name" | tr -d '\r')"
  size="$(xml_bytes "$xml" size)"
  block="$(xml_bytes "$xml" block)"
  requested="$(xml_bytes "$xml" requested)"
  current="$(xml_bytes "$xml" current)"
  qga_free="-"
  qga_total="-"
  if qga_response="$(virsh_command qemu-agent-command "$vm_name" '{"execute":"guest-get-memory-stats"}' 2>/dev/null)"; then
    qga_free="$(printf '%s\n' "$qga_response" | jq -r '[.return[]? | select(.stat == "stat-free") | .value][0] // "-"')"
    qga_total="$(printf '%s\n' "$qga_response" | jq -r '[.return[]? | select(.stat == "stat-total") | .value][0] // "-"')"
  fi
  timestamp="$(date --iso-8601=seconds)"
  printf 'sample timestamp=%s state=%s requested=%s current=%s size=%s block=%s qga_free=%s qga_total=%s\n' \
    "$timestamp" "$state" "$requested" "$current" "$size" "$block" "$qga_free" "$qga_total"
  if [[ -n "$log_path" ]]; then
    printf '%s,%s,%s,%s,%s,%s,%s,%s\n' \
      "$timestamp" "$state" "$requested" "$current" "$size" "$block" "$qga_free" "$qga_total" >>"$log_path"
  fi
  SAMPLE_REQUESTED="$requested"
  SAMPLE_CURRENT="$current"
}

xml_bytes() {
  local xml="$1"
  local field="$2"
  local value unit factor
  value="$(xmllint --xpath "string(($memory_xpath//*[local-name()='$field'])[1])" - <<<"$xml")"
  unit="$(xmllint --xpath "string(($memory_xpath//*[local-name()='$field']/@unit)[1])" - <<<"$xml")"
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

memory_xpath="/domain/devices/memory[@model='virtio-mem'][alias[@name='$alias']]"
xml="$(virsh_command dumpxml "$vm_name")"
if [[ "$(xmllint --xpath "count($memory_xpath)" - <<<"$xml")" != "1" ]]; then
  printf 'Expected exactly one virtio-mem device with alias %s.\n' "$alias" >&2
  exit 20
fi

size="$(xml_bytes "$xml" size)"
block="$(xml_bytes "$xml" block)"
original_requested="$(xml_bytes "$xml" requested)"
original_current="$(xml_bytes "$xml" current)"

target="$target_bytes"
if (( block < 1048576 || (block & (block - 1)) != 0 )); then
  printf 'BLOCKED: block size must be a power of two of at least 1 MiB.\n' >&2
  exit 20
fi
if (( target > size || target % block != 0 )); then
  printf 'BLOCKED: target must be within device size and aligned to block size.\n' >&2
  exit 20
fi
if (( original_requested != original_current )); then
  printf 'BLOCKED: existing request has not converged (requested=%s current=%s).\n' "$original_requested" "$original_current" >&2
  exit 20
fi

log "baseline vm=$vm_name alias=$alias requested=$original_requested current=$original_current size=$size block=$block target=$target"
if (( target == original_current )); then
  log 'NO CHANGE: target equals current memory.'
  exit 0
fi
if (( apply == 0 )); then
  log 'DRY RUN: target is valid, but --apply was not supplied.'
  exit 0
fi

mutation_started=0
rollback_done=0
rollback_target="$original_current"
wait_for_target() {
  local expected="$1"
  local deadline=$((SECONDS + timeout_seconds))
  while (( SECONDS < deadline )); do
    record_sample
    if (( SAMPLE_REQUESTED == expected && SAMPLE_CURRENT == expected )); then
      return 0
    fi
    sleep "$interval_seconds"
  done
  record_sample
  printf 'TIMEOUT: expected requested=current=%s within %s seconds.\n' "$expected" "$timeout_seconds" >&2
  return 1
}
rollback() {
  if (( mutation_started == 1 && rollback_done == 0 && keep_target == 0 )); then
    rollback_done=1
    log "ROLLBACK: requesting original size $rollback_target bytes."
    virsh_command update-memory-device "$vm_name" \
      --alias "$alias" \
      --requested-size "$rollback_target" \
      --live >/dev/null
    wait_for_target "$rollback_target" || true
  fi
}
trap rollback EXIT INT TERM

log "APPLY: requesting target $target bytes."
virsh_command update-memory-device "$vm_name" \
  --alias "$alias" \
  --requested-size "$target" \
  --live
mutation_started=1
if ! wait_for_target "$target"; then
  exit 1
fi
log "CONVERGED: target $target bytes reached."
if (( keep_target == 1 )); then
  log 'KEEP: target retained by explicit --keep-target.'
else
  rollback
  if (( rollback_done == 1 )); then
    log "RESTORED: original size $rollback_target bytes requested."
  fi
fi
