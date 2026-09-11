use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use virtio_mem_core::{parse_virtio_mem_xml_for_alias, VirtioMemState};
use virtio_mem_host::attestation::AttestedCompatibilitySource;
use virtio_mem_host::compatibility_source::CompatibilitySource;
use virtio_mem_host::qga::VirshQgaFileReader;
use virtio_mem_host::raw_telemetry::{
    QgaFileRawTelemetrySource, RawTelemetryRead, RawTelemetrySource,
};
use virtio_mem_host::virsh::Virsh;

use crate::{process, windows};

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    vm: String,
    alias: String,
    unit: String,
    telemetry_path: String,
    service: String,
    attestation: PathBuf,
    host_deployment_evidence: PathBuf,
    ack_path: PathBuf,
    windows_evidence: PathBuf,
    output: PathBuf,
    host_headroom_bytes: u64,
    sample_interval: Duration,
    telemetry_max_age: Duration,
    future_tolerance: Duration,
    preflight_timeout: Duration,
    command_timeout: Duration,
    connection: String,
    apply_service_restart: bool,
}

#[derive(Debug, Serialize)]
struct Evidence {
    schema_version: u32,
    vm_name: String,
    device_alias: String,
    controller_unit: String,
    controller_active_state: String,
    controller_unit_file_state: String,
    host_mem_available_bytes: u64,
    required_host_headroom_bytes: u64,
    requested_before_bytes: u64,
    current_before_bytes: u64,
    requested_after_bytes: u64,
    current_after_bytes: u64,
    initial_session_id: String,
    initial_sequence: u64,
    initial_restart_read_unchanged: bool,
    new_session_id: String,
    new_session_sequence: u64,
    new_session_restart_read_unchanged: bool,
    windows_service_restart_applied: bool,
    attestation_current: bool,
    rollback_path: String,
    no_memory_mutation_observed: bool,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.len() < 2 {
        return Err("preflight requires VM_NAME ALIAS and explicit options".to_owned());
    }
    validate_identity(&args[0], "VM_NAME")?;
    validate_identity(&args[1], "ALIAS")?;
    let mut values = std::collections::BTreeMap::new();
    let mut apply_service_restart = false;
    let mut index = 2;
    while index < args.len() {
        let option = args[index].clone();
        if option == "--apply-service-restart" && !apply_service_restart {
            apply_service_restart = true;
            index += 1;
            continue;
        }
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("{option} requires a value"))?;
        if !matches!(
            option.as_str(),
            "--unit"
                | "--telemetry-path"
                | "--service"
                | "--attestation"
                | "--host-deployment-evidence"
                | "--ack-path"
                | "--windows-evidence"
                | "--output"
                | "--host-headroom-bytes"
                | "--sample-interval-seconds"
                | "--telemetry-max-age-seconds"
                | "--future-tolerance-seconds"
                | "--preflight-timeout-seconds"
                | "--command-timeout-seconds"
                | "--connect"
        ) || values.insert(option.clone(), value.clone()).is_some()
        {
            return Err(format!("unknown or duplicate preflight option: {option}"));
        }
        index += 1;
    }
    let required = |name: &str| {
        values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("preflight requires {name}"))
    };
    let positive = |name: &str| -> Result<u64, String> {
        required(name)?
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{name} requires a positive integer"))
    };
    let unit = required("--unit")?;
    let expected_unit = format!("virtio-mem-host@{}.service", args[0]);
    if unit != expected_unit {
        return Err(format!("--unit must be {expected_unit}"));
    }
    let telemetry_path = required("--telemetry-path")?;
    if !telemetry_path
        .as_bytes()
        .get(1)
        .is_some_and(|byte| *byte == b':')
        || !telemetry_path.contains('\\')
        || telemetry_path.contains("..")
    {
        return Err("--telemetry-path must be an absolute safe Windows path".to_owned());
    }
    Ok(Command {
        vm: args[0].clone(),
        alias: args[1].clone(),
        unit,
        telemetry_path,
        service: required("--service")?,
        attestation: PathBuf::from(required("--attestation")?),
        host_deployment_evidence: PathBuf::from(required("--host-deployment-evidence")?),
        ack_path: PathBuf::from(required("--ack-path")?),
        windows_evidence: PathBuf::from(required("--windows-evidence")?),
        output: PathBuf::from(required("--output")?),
        host_headroom_bytes: positive("--host-headroom-bytes")?,
        sample_interval: Duration::from_secs(positive("--sample-interval-seconds")?),
        telemetry_max_age: Duration::from_secs(positive("--telemetry-max-age-seconds")?),
        future_tolerance: Duration::from_secs(positive("--future-tolerance-seconds")?),
        preflight_timeout: Duration::from_secs(positive("--preflight-timeout-seconds")?),
        command_timeout: Duration::from_secs(positive("--command-timeout-seconds")?),
        connection: values
            .get("--connect")
            .cloned()
            .unwrap_or_else(|| "qemu:///system".to_owned()),
        apply_service_restart,
    })
}

pub fn run(command: &Command, repo: &Path) -> Result<(), String> {
    if !command.apply_service_restart {
        return Err(
            "QA-T008 requires --apply-service-restart; no memory mutation is performed".to_owned(),
        );
    }
    let unit_state = controller_state(command, repo)?;
    let before = live_state(command, repo)?;
    require_converged(before)?;
    let host_mem_available_bytes = host_mem_available()?;
    if host_mem_available_bytes < command.host_headroom_bytes {
        return Err(format!(
            "host headroom is below the explicit bound (available={host_mem_available_bytes}, required={})",
            command.host_headroom_bytes
        ));
    }
    let attestation = resolve(repo, &command.attestation);
    AttestedCompatibilitySource::new(
        Virsh::with_connection("virsh", command.command_timeout, &command.connection),
        &command.vm,
        &command.alias,
        attestation,
    )
    .compatibility()?
    .validate_for_resize()
    .map_err(|error| format!("preflight attestation failed: {error}"))?;
    let rollback_path = applied_rollback_path(&resolve(repo, &command.host_deployment_evidence))?;
    let ack_path = resolve(repo, &command.ack_path);
    if ack_path.exists() {
        return Err(format!(
            "preflight ack path already exists: {}",
            ack_path.display()
        ));
    }
    if let Some(parent) = ack_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create preflight artifact directory: {error}"))?;
    }

    let initial_source = source(command, &ack_path);
    let initial = expect_fresh(initial_source.read()?, "initial telemetry")?;
    let restarted_source = source(command, &ack_path);
    let initial_restart =
        expect_unchanged(restarted_source.read()?, "initial host-reader restart")?;
    if initial.session_id != initial_restart.session_id
        || initial.sequence != initial_restart.sequence
    {
        return Err(
            "durable acknowledgement did not preserve the initial record identity".to_owned(),
        );
    }

    windows::run(
        windows::Command::ServiceCycle {
            service: command.service.clone(),
            output: command.windows_evidence.clone(),
            apply: true,
        },
        repo,
    )?;

    let deadline = Instant::now() + command.preflight_timeout;
    let new_session = loop {
        let sample = restarted_source.read()?;
        if let RawTelemetryRead::Fresh(envelope) = sample {
            if envelope.session_id != initial.session_id {
                break envelope;
            }
        }
        if Instant::now() >= deadline {
            return Err(
                "Windows telemetry did not roll to a new session before the preflight deadline"
                    .to_owned(),
            );
        }
        std::thread::sleep(command.sample_interval);
    };
    let final_source = source(command, &ack_path);
    let final_restart = expect_unchanged(final_source.read()?, "new-session host-reader restart")?;
    if final_restart.session_id != new_session.session_id
        || final_restart.sequence != new_session.sequence
    {
        return Err(
            "durable acknowledgement did not preserve the new-session record identity".to_owned(),
        );
    }
    let after = live_state(command, repo)?;
    require_converged(after)?;
    if before != after {
        return Err(format!("memory state changed during no-actuation preflight: before={before:?}, after={after:?}"));
    }
    let final_unit_state = controller_state(command, repo)?;
    if final_unit_state != unit_state {
        return Err("controller unit state changed during preflight".to_owned());
    }
    persist(
        &resolve(repo, &command.output),
        &Evidence {
            schema_version: 1,
            vm_name: command.vm.clone(),
            device_alias: command.alias.clone(),
            controller_unit: command.unit.clone(),
            controller_active_state: unit_state.0,
            controller_unit_file_state: unit_state.1,
            host_mem_available_bytes,
            required_host_headroom_bytes: command.host_headroom_bytes,
            requested_before_bytes: before.requested_bytes,
            current_before_bytes: before.current_bytes,
            requested_after_bytes: after.requested_bytes,
            current_after_bytes: after.current_bytes,
            initial_session_id: initial.session_id,
            initial_sequence: initial.sequence,
            initial_restart_read_unchanged: true,
            new_session_id: new_session.session_id,
            new_session_sequence: new_session.sequence,
            new_session_restart_read_unchanged: true,
            windows_service_restart_applied: true,
            attestation_current: true,
            rollback_path,
            no_memory_mutation_observed: true,
        },
    )?;
    println!(
        "No-actuation preflight evidence written to {}",
        resolve(repo, &command.output).display()
    );
    Ok(())
}

fn source(
    command: &Command,
    ack_path: &Path,
) -> QgaFileRawTelemetrySource<VirshQgaFileReader<Virsh>> {
    QgaFileRawTelemetrySource::new(
        VirshQgaFileReader::new(
            Virsh::with_connection("virsh", command.command_timeout, &command.connection),
            &command.vm,
        ),
        &command.telemetry_path,
        ack_path,
        &command.vm,
        &command.service,
        command.telemetry_max_age,
        command.future_tolerance,
    )
}

fn expect_fresh(
    read: RawTelemetryRead,
    description: &str,
) -> Result<virtio_mem_core::RawTelemetryEnvelope, String> {
    match read {
        RawTelemetryRead::Fresh(value) => Ok(value),
        RawTelemetryRead::Unchanged(_) => Err(format!("{description} was unexpectedly unchanged")),
    }
}
fn expect_unchanged(
    read: RawTelemetryRead,
    description: &str,
) -> Result<virtio_mem_core::RawTelemetryEnvelope, String> {
    match read {
        RawTelemetryRead::Unchanged(value) => Ok(value),
        RawTelemetryRead::Fresh(_) => Err(format!("{description} replayed a fresh record")),
    }
}

fn live_state(command: &Command, repo: &Path) -> Result<VirtioMemState, String> {
    let xml = process::bounded_text(
        "virsh",
        &[
            process::os("--connect"),
            process::os(&command.connection),
            process::os("dumpxml"),
            process::os(&command.vm),
        ],
        repo,
        command.command_timeout,
    )?;
    parse_virtio_mem_xml_for_alias(&xml, &command.alias)
        .map(|state| state.memory)
        .map_err(|error| format!("preflight live state: {error}"))
}
fn require_converged(state: VirtioMemState) -> Result<(), String> {
    if state.requested_bytes == state.current_bytes {
        Ok(())
    } else {
        Err(format!(
            "preflight requires requested == current: {state:?}"
        ))
    }
}

fn controller_state(command: &Command, repo: &Path) -> Result<(String, String), String> {
    let text = process::bounded_text(
        "systemctl",
        &[
            process::os("show"),
            process::os(&command.unit),
            process::os("--property=ActiveState,UnitFileState,MainPID"),
            process::os("--no-pager"),
        ],
        repo,
        command.command_timeout,
    )?;
    let fields = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect::<std::collections::BTreeMap<_, _>>();
    let active = fields.get("ActiveState").copied().unwrap_or("");
    let enabled = fields.get("UnitFileState").copied().unwrap_or("");
    let pid = fields.get("MainPID").copied().unwrap_or("");
    if active != "inactive" || enabled != "disabled" || pid != "0" {
        return Err(format!(
            "controller is not in disabled inactive safe hold: {text:?}"
        ));
    }
    Ok((active.to_owned(), enabled.to_owned()))
}

fn host_mem_available() -> Result<u64, String> {
    let text = std::fs::read_to_string("/proc/meminfo")
        .map_err(|error| format!("read /proc/meminfo: {error}"))?;
    let kib = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .ok_or_else(|| "MemAvailable is missing".to_owned())?;
    kib.checked_mul(1024)
        .ok_or_else(|| "MemAvailable overflows bytes".to_owned())
}

fn applied_rollback_path(path: &Path) -> Result<String, String> {
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .map_err(|error| format!("read host deployment evidence: {error}"))?,
    )
    .map_err(|error| format!("parse host deployment evidence: {error}"))?;
    if document.get("applied").and_then(Value::as_bool) != Some(true) {
        return Err("host deployment evidence is not applied".to_owned());
    }
    document
        .get("backup_path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "host deployment evidence lacks rollback path".to_owned())
}

fn persist(path: &Path, evidence: &Evidence) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "preflight output has no parent".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create preflight evidence directory: {error}"))?;
    let mut json = serde_json::to_vec_pretty(evidence)
        .map_err(|error| format!("serialize preflight evidence: {error}"))?;
    json.push(b'\n');
    let temporary = parent.join(format!(".preflight-{}.tmp", std::process::id()));
    std::fs::write(&temporary, json)
        .map_err(|error| format!("write preflight evidence: {error}"))?;
    std::fs::rename(&temporary, path).map_err(|error| format!("commit preflight evidence: {error}"))
}

fn resolve(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}
fn validate_identity(value: &str, name: &str) -> Result<(), String> {
    if !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        Ok(())
    } else {
        Err(format!("{name} contains unsafe characters"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_implicit_preflight_scope() {
        assert!(parse(&["guest".to_owned(), "alias".to_owned()]).is_err());
    }
}
