#!/bin/bash
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
Usage: scripts/complete-windows-build-milestone.sh SSH_TARGET EXPECTED_ED25519_FINGERPRINT [IDENTITY_FILE]

Checks the pinned Windows SSH host key, verifies the remote MSVC toolchain,
runs the complete RHEL and Windows gate twice, and records logs and artifact
SHA-256 values under .vscode-artifacts/windows/milestone-TIMESTAMP/.

SSH_TARGET may be an SSH config alias or USER@HOST. IDENTITY_FILE is optional
when the key is already selected by ssh-agent or the user's SSH configuration.
EOF
    exit 2
}

ssh_target="${1:-}"
expected_fingerprint="${2:-}"
identity_file="${3:-}"

[[ -n "$ssh_target" && -n "$expected_fingerprint" ]] || usage

case "$ssh_target" in
    -* | *[$'\r\n']*)
        printf 'SSH_TARGET must be an SSH alias or USER@HOST, not an option or command.\n' >&2
        exit 2
        ;;
esac

case "$expected_fingerprint" in
    SHA256:[A-Za-z0-9+/]*) ;;
    *)
        printf 'Expected an OpenSSH SHA256 fingerprint.\n' >&2
        exit 2
        ;;
esac

if [[ -n "$identity_file" && ! -f "$identity_file" ]]; then
    printf 'SSH identity file does not exist: %s\n' "$identity_file" >&2
    exit 2
fi

required_commands=(make ssh ssh-keygen ssh-keyscan sha256sum tee)
for command_name in "${required_commands[@]}"; do
    command -v "$command_name" >/dev/null || {
        printf 'Missing required command: %s\n' "$command_name" >&2
        exit 1
    }
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
evidence_dir="$repo_root/.vscode-artifacts/windows/milestone-$timestamp"
summary_file="$evidence_dir/summary.txt"
known_hosts_file="$(mktemp "${TMPDIR:-/tmp}/virtio-mem-known-hosts.XXXXXX")"
trap 'rm -f -- "$known_hosts_file"' EXIT

ssh_configuration="$(ssh -G -- "$ssh_target")"
host_name="$(awk '$1 == "hostname" { print $2; exit }' <<<"$ssh_configuration")"
host_port="$(awk '$1 == "port" { print $2; exit }' <<<"$ssh_configuration")"

if [[ -z "$host_name" || -z "$host_port" ]]; then
    printf 'Could not resolve the SSH host and port for: %s\n' "$ssh_target" >&2
    exit 1
fi

ssh-keyscan -T 10 -p "$host_port" -t ed25519 -- "$host_name" \
    >"$known_hosts_file" 2>/dev/null

observed_fingerprint="$(ssh-keygen -E sha256 -lf "$known_hosts_file" | awk 'NR == 1 { print $2 }')"
if [[ "$observed_fingerprint" != "$expected_fingerprint" ]]; then
    printf 'Windows SSH host fingerprint mismatch (expected=%s, observed=%s).\n' \
        "$expected_fingerprint" "${observed_fingerprint:-missing}" >&2
    exit 1
fi

mkdir -p "$evidence_dir"
export VIRTIO_MEM_WINDOWS_SSH="$ssh_target"
export VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE="$known_hosts_file"
if [[ -n "$identity_file" ]]; then
    export VIRTIO_MEM_WINDOWS_IDENTITY_FILE="$identity_file"
fi

{
    printf 'UTC run timestamp: %s\n' "$timestamp"
    printf 'Windows SSH target: %s\n' "$ssh_target"
    printf 'Verified Windows SSH host fingerprint: %s\n' "$observed_fingerprint"
    printf 'Recording milestone evidence in: %s\n' "$evidence_dir"
} | tee "$summary_file"

cd "$repo_root"
bash scripts/windows-remote-build.sh check 2>&1 | tee "$evidence_dir/endpoint-check.log"

for run_number in 1 2; do
    log_file="$evidence_dir/all-gates-run-$run_number.log"
    hash_file="$evidence_dir/artifact-run-$run_number.sha256"
    printf 'Starting aggregate gate %s of 2.\n' "$run_number"
    make all-gates 2>&1 | tee "$log_file"
    sha256sum .vscode-artifacts/windows/virtio-mem-service.exe | tee "$hash_file"
done

printf 'Two consecutive aggregate gates passed. Evidence: %s\n' "$evidence_dir" \
    | tee -a "$summary_file"
