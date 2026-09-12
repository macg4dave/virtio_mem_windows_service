mod attestation;
mod calibration;
mod deployment;
mod host_deploy;
mod live_resize;
mod local;
mod preflight;
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
  cargo xtask deployment inventory INSTANCE [OPTIONS]
  cargo xtask calibration VM_NAME ALIAS [OPTIONS]
  cargo xtask attestation VM_NAME ALIAS REVIEW --output PATH [OPTIONS]
  cargo xtask host-deploy INSTANCE --config PATH --attestation PATH [OPTIONS]
  cargo xtask preflight VM_NAME ALIAS [OPTIONS]
  cargo xtask windows <check|sync|build|test|lint|fetch|all>
  cargo xtask windows deploy MANIFEST --output PATH [--apply]
  cargo xtask windows service-cycle SERVICE --output PATH [--apply]
  cargo xtask windows diagnose-service SERVICE --output PATH
  cargo xtask windows verify EXPECTED_ED25519_FINGERPRINT --runs N
  cargo xtask qga VM_NAME --attempts N --command-timeout-seconds N [--connect URI]
  cargo xtask live-resize VM_NAME ALIAS TARGET_BYTES [OPTIONS]
  cargo xtask qualification <start|run|status|review> [OPTIONS]

Deployment inventory options:
  --unit UNIT                   Required exact inactive systemd instance unit.
  --binary PATH                 Required absolute installed executable path.
  --text-file PATH              Required absolute inspected file; repeatable.
  --output PATH                 Required structured JSON evidence path.
  --command-timeout-seconds N   Required external-command bound.
  --elevate                     Invoke this prebuilt xtask once through sudo.

Calibration options:
  --telemetry-path PATH         Required absolute Windows telemetry file path.
  --output PATH                 Required structured JSON evidence path.
  --sample-interval-seconds N   Required interval between two samples.
  --max-age-seconds N           Required telemetry freshness bound.
  --future-tolerance-seconds N  Required future-clock tolerance.
  --command-timeout-seconds N   Required external-command bound.
  --connect URI                 Libvirt URI; default qemu:///system.

Attestation options:
  --output PATH                 Required generated attestation path.
  --command-timeout-seconds N   Required external-command bound.
  --connect URI                 Libvirt URI; default qemu:///system.

Host deployment options:
  --config PATH                 Required reviewed instance environment file.
  --attestation PATH            Required current compatibility attestation.
  --output PATH                 Required structured JSON evidence path.
  --command-timeout-seconds N   Required external-command bound.
  --apply                       Install; otherwise validate only.
  --elevate                     Invoke this prebuilt xtask once through sudo.

No-actuation preflight options:
  --unit UNIT                   Required disabled/inactive controller unit.
  --telemetry-path PATH         Required protected Windows telemetry path.
  --service SERVICE             Required Windows telemetry service identity.
  --attestation PATH            Required reviewed attestation.
  --host-deployment-evidence PATH  Required applied host deployment evidence.
  --ack-path PATH               Required new host replay-state evidence path.
  --windows-evidence PATH       Required Windows restart evidence path.
  --output PATH                 Required structured result path.
  --host-headroom-bytes N       Required minimum MemAvailable.
  --sample-interval-seconds N   Required poll interval.
  --telemetry-max-age-seconds N Required telemetry freshness bound.
  --future-tolerance-seconds N  Required future-clock tolerance.
  --preflight-timeout-seconds N Required overall new-session bound.
  --command-timeout-seconds N   Required external-command bound.
  --connect URI                 Libvirt URI; default qemu:///system.
  --apply-service-restart       Restart only the named Windows service.

Qualification commands:
  start VM ALIAS --ssh-target TARGET --mode resident|committed [--apply] [OPTIONS]
  status RUN_ID [--output-root PATH]
  review RUN_ID [--output-root PATH]

Qualification options:
  --apply                    Start the workload; otherwise validate only.
  --elevate                  Use one sudo boundary for bounded controller start/stop.
  --mode MODE                Required resident or committed workload behavior.
  --peak-bytes N             Required peak allocation.
  --retained-bytes N         Required allocation retained between peaks.
  --max-allocation-bytes N   Required independent workload safety cap.
  --peak-hold-seconds N      Required first-peak hold.
  --settled-hold-seconds N   Required retained-allocation hold.
  --renewed-hold-seconds N   Required renewed-peak hold.
  --resident-refresh-seconds N  Required page refresh interval.
  --post-hold-seconds N      Required final observation window; may be zero.
  --interval-seconds N       Required host sampling interval.
  --command-timeout-seconds N  Required bound for external commands.
  --controller-timeout-seconds N Required hard bound for controller ownership.
  --expect-growth-bytes N    Required minimum observed growth.
  --expect-reclaim-bytes N   Required minimum observed reclaim.
  --remote-workload PATH     Required Windows workload executable path.
  --controller-unit UNIT     Required host systemd unit to archive.
  --guest-service NAME       Required Windows service identity.
  --telemetry-path PATH      Required protected Windows telemetry path read through QGA.
  --telemetry-max-age-seconds N Required telemetry freshness bound.
  --telemetry-future-tolerance-seconds N Required future-clock tolerance.
  --connect URI              Libvirt URI; default qemu:///system.
  --output-root PATH         Default .artifacts/qualification.

Live-resize options:
  --apply                 Issue the explicitly requested live resize.
  --keep-target           Do not restore; requires --apply and explicit approval.
  --forward-timeout-seconds N   Required forward convergence bound.
  --rollback-timeout-seconds N  Required restoration convergence bound.
  --sample-interval-seconds N   Required sampling interval.
  --command-timeout-seconds N  Required external-command bound.
  --connect URI           Libvirt URI; default qemu:///system.
  --minimum-target-bytes N     Required deployment safety floor.
  --host-min-headroom-bytes N Required post-growth MemAvailable reserve.
  --attestation PATH      Required reviewed compatibility attestation.
  --host-cli PATH         Required attestation-aware host product binary.
  --log PATH              Append detailed CSV samples.

Windows environment:
  VIRTIO_MEM_WINDOWS_SSH              Required SSH config alias or USER@HOST.
  VIRTIO_MEM_WINDOWS_DIR              Remote workspace directory.
  VIRTIO_MEM_WINDOWS_ARTIFACTS        Local artifact directory.
  VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS Required SSH connection bound.
  VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS Required operation bound.
  VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE Optional pinned known-hosts file.
  VIRTIO_MEM_WINDOWS_IDENTITY_FILE    Optional private-key path.
  VIRTIO_MEM_WINDOWS_HOST_NAME        Optional current endpoint override.
  VIRTIO_MEM_WINDOWS_HOST_KEY_ALIAS   Required with host-name override.
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
        Some("deployment") => deployment::run(&deployment::parse(&arguments[1..])?, &repo),
        Some("calibration") => calibration::run(&calibration::parse(&arguments[1..])?, &repo),
        Some("attestation") => attestation::run(&attestation::parse(&arguments[1..])?, &repo),
        Some("host-deploy") => host_deploy::run(&host_deploy::parse(&arguments[1..])?, &repo),
        Some("preflight") => preflight::run(&preflight::parse(&arguments[1..])?, &repo),
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
