#!/bin/bash
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
Usage: scripts/windows-remote-build.sh <check|sync|build|test|lint|fetch|all>

Required environment:
  VIRTIO_MEM_WINDOWS_SSH   SSH config alias for the Windows build guest

Optional environment:
  VIRTIO_MEM_WINDOWS_DIR   Windows workspace path (default: C:\Users\Public\virtio-mem-build)
  VIRTIO_MEM_WINDOWS_ARTIFACTS  Local artifact directory (default: .vscode-artifacts/windows)
EOF
    exit 2
}

operation="${1:-}"
case "$operation" in
    check|sync|build|test|lint|fetch|all) ;;
    *) usage ;;
esac

ssh_alias="${VIRTIO_MEM_WINDOWS_SSH:-}"
if [[ -z "$ssh_alias" ]]; then
    printf 'VIRTIO_MEM_WINDOWS_SSH is required; refusing to guess a guest.\n' >&2
    exit 2
fi

remote_dir="${VIRTIO_MEM_WINDOWS_DIR:-C:\\Users\\Public\\virtio-mem-build}"
artifact_dir="${VIRTIO_MEM_WINDOWS_ARTIFACTS:-.vscode-artifacts/windows}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive="$(mktemp "${TMPDIR:-/tmp}/virtio-mem-windows.XXXXXX.tar")"
trap 'rm -f "$archive"' EXIT

if [[ "$artifact_dir" != /* ]]; then
    artifact_dir="$repo_root/$artifact_dir"
fi

case "$ssh_alias" in
    -* | *[$'\r\n']*)
        printf 'VIRTIO_MEM_WINDOWS_SSH must be an SSH config alias, not an option or command.\n' >&2
        exit 2
        ;;
esac

case "$remote_dir" in
    *['&|<>^%!']* | *[$'\r\n']*)
        printf 'VIRTIO_MEM_WINDOWS_DIR contains characters that are unsafe for cmd.exe.\n' >&2
        exit 2
        ;;
esac

remote() {
    ssh -- "$ssh_alias" "cmd.exe /d /c $1"
}

remote_quoted_path="\"${remote_dir//\"/}\""

check() {
    command -v ssh >/dev/null
    command -v scp >/dev/null
    command -v tar >/dev/null
    command -v sha256sum >/dev/null
    remote "where rustc && rustc --version && cargo --version && rustup target list --installed && rustup component list --installed && where tar && where certutil && if not exist \"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe\" exit /b 1"
    with_msvc 'where link'
}

sync_source() {
    git -C "$repo_root" ls-files --cached --others --exclude-standard -z |
        tar -C "$repo_root" --null --files-from=- -cf "$archive"
    remote "if not exist $remote_quoted_path mkdir $remote_quoted_path"
    scp -- "$archive" "${ssh_alias}:virtio-mem-windows-source.tar"
    remote "tar -xf virtio-mem-windows-source.tar -C $remote_quoted_path && del /q virtio-mem-windows-source.tar"
}

with_msvc() {
    local command_text="$1"
    remote "for /f \"delims=\" %i in ('\"%ProgramFiles(x86)%\\Microsoft Visual Studio\\Installer\\vswhere.exe\" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find VC\\Auxiliary\\Build\\vcvars64.bat') do @call \"%i\" ^&^& cd /d $remote_quoted_path ^&^& $command_text"
}

build_windows() {
    with_msvc 'if exist target\release\virtio-mem-service.exe del /q target\release\virtio-mem-service.exe ^& cargo build -p virtio-mem-service --release --locked'
}

test_windows() {
    with_msvc 'cargo test -p virtio-mem-service --all-features --locked'
}

lint_windows() {
    with_msvc 'cargo fmt --all -- --check ^&^& cargo clippy -p virtio-mem-service --all-targets --all-features --locked -- -D warnings'
}

fetch_artifact() {
    mkdir -p "$artifact_dir"
    local remote_artifact="${remote_dir//\\/\/}/target/release/virtio-mem-service.exe"
    scp -- "${ssh_alias}:${remote_artifact}" "$artifact_dir/virtio-mem-service.exe"
    local remote_hash local_hash
    remote_hash="$(remote "certutil -hashfile $remote_quoted_path\\target\\release\\virtio-mem-service.exe SHA256" | awk '/^[0-9A-Fa-f]{64}$/ { print tolower($0); exit }')"
    local_hash="$(sha256sum "$artifact_dir/virtio-mem-service.exe" | awk '{print $1}')"
    if [[ -z "$remote_hash" || "$remote_hash" != "$local_hash" ]]; then
        printf 'Artifact checksum mismatch (Windows=%s, RHEL=%s).\n' "${remote_hash:-missing}" "$local_hash" >&2
        return 1
    fi
    printf 'Windows artifact verified: %s (%s)\n' \
        "$artifact_dir/virtio-mem-service.exe" "$local_hash"
}

case "$operation" in
    check) check ;;
    sync) sync_source ;;
    build) build_windows ;;
    test) test_windows ;;
    lint) lint_windows ;;
    fetch) fetch_artifact ;;
    all)
        check
        sync_source
        build_windows
        test_windows
        lint_windows
        fetch_artifact
        ;;
esac
