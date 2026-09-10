mod live_resize;
mod local;
mod process;
mod qga;
mod qualification;
mod windows;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = r#"Repository build and test tooling

Usage:
  cargo xtask gate <format|build|test|lint|local|all>
  cargo xtask doctor host
  cargo xtask windows <check|sync|build|test|lint|fetch|all>
  cargo xtask windows milestone SSH_TARGET EXPECTED_ED25519_FINGERPRINT [IDENTITY_FILE]
  cargo xtask qga VM_NAME [--attempts N] [--connect URI]
  cargo xtask live-resize VM_NAME ALIAS TARGET_BYTES [OPTIONS]
  cargo xtask qualification <start|run|status|review> [OPTIONS]

Qualification commands:
  start VM ALIAS --ssh-target TARGET --profile m10g-resident [--apply] [OPTIONS]
  status RUN_ID [--output-root PATH]
  review RUN_ID [--output-root PATH]

Qualification options:
  --profile m10g-resident|m10g-committed
  --apply                    Start the workload; otherwise validate only.
  --peak-bytes N             Default 4 GiB.
  --retained-bytes N         Default 2 GiB.
  --peak-hold-seconds N      Default 600.
  --settled-hold-seconds N   Default 900.
  --renewed-hold-seconds N   Default 600.
  --post-hold-seconds N      Final observation window; default 60.
  --interval-seconds N       Host sampling interval; default 5.
  --expect-growth-bytes N    Minimum observed growth; default 1 GiB.
  --expect-reclaim-bytes N   Minimum observed reclaim; default 64 MiB.
  --remote-workload PATH     Windows workload executable path.
  --controller-unit UNIT     Host systemd unit to archive.
  --telemetry-path PATH      Optional host-side raw Windows telemetry file.
  --connect URI              Libvirt URI; default qemu:///system.
  --output-root PATH         Default .vscode-artifacts/qualification.

Live-resize options:
  --apply                 Issue the explicitly requested live resize.
  --keep-target           Do not restore; requires --apply and explicit approval.
  --timeout SECONDS       Forward convergence timeout, maximum 30; default 30.
  --rollback-timeout N    Restoration convergence timeout; default 300.
  --interval SECONDS      Sampling interval; default 5.
  --connect URI           Libvirt URI; default qemu:///system.
  --max-target-bytes N    Safety cap; default 8 GiB.
  --host-reserve-bytes N  Required post-growth MemAvailable; default 4 GiB.
  --log PATH              Append detailed CSV samples.

Windows environment:
  VIRTIO_MEM_WINDOWS_SSH              Required SSH config alias or USER@HOST.
  VIRTIO_MEM_WINDOWS_DIR              Remote workspace directory.
  VIRTIO_MEM_WINDOWS_ARTIFACTS        Local artifact directory.
  VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE Optional pinned known-hosts file.
  VIRTIO_MEM_WINDOWS_IDENTITY_FILE    Optional private-key path.
"#;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match execute(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("virtio-mem-xtask failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(arguments: &[String]) -> Result<(), String> {
    let repo = repo_root()?;
    match arguments.first().map(String::as_str) {
        Some("help" | "--help" | "-h") | None => {
            print!("{HELP}");
            Ok(())
        }
        Some("gate") => gate(&arguments[1..], &repo),
        Some("doctor")
            if arguments.get(1).map(String::as_str) == Some("host") && arguments.len() == 2 =>
        {
            local::doctor_host()
        }
        Some("doctor") => Err("doctor requires exactly: host".to_owned()),
        Some("windows") => windows::run(windows::parse(&arguments[1..])?, &repo),
        Some("qga") => qga::run(&qga::parse(&arguments[1..])?, &repo),
        Some("live-resize") => live_resize::run(&live_resize::parse(&arguments[1..])?, &repo),
        Some("qualification") => qualification::execute(&arguments[1..], &repo),
        Some(command) => Err(format!("unknown command: {command}\n\n{HELP}")),
    }
}

fn gate(arguments: &[String], repo: &Path) -> Result<(), String> {
    if arguments.len() != 1 {
        return Err("gate requires format|build|test|lint|local|all".to_owned());
    }
    let step = match arguments[0].as_str() {
        "format" => local::Step::Format,
        "build" => local::Step::Build,
        "test" => local::Step::Test,
        "lint" => local::Step::Lint,
        "local" => local::Step::Local,
        "all" => {
            local::run(local::Step::Local, repo)?;
            return windows::run_all(repo);
        }
        value => return Err(format!("unknown gate: {value}")),
    };
    local::run(step, repo)
}

fn repo_root() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "xtask manifest is not nested under the repository root".to_owned())?;
    Ok(root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_root_contains_workspace_manifest() {
        assert!(repo_root()
            .expect("repository root")
            .join("Cargo.toml")
            .is_file());
    }
}
