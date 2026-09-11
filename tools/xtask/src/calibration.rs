use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use virtio_mem_core::{parse_virtio_mem_xml_for_alias, RawTelemetryEnvelope};
use virtio_mem_host::qga::{GuestFileReader, VirshQgaFileReader};
use virtio_mem_host::virsh::Virsh;

use crate::process;

#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    vm: String,
    alias: String,
    telemetry_path: String,
    output: PathBuf,
    sample_interval: Duration,
    max_age: Duration,
    future_tolerance: Duration,
    command_timeout: Duration,
    connection: String,
}

#[derive(Debug, Serialize)]
struct Evidence {
    schema_version: u32,
    captured_unix_millis: u64,
    vm_name: String,
    device_alias: String,
    telemetry_path: String,
    block_size_bytes: u64,
    device_size_bytes: u64,
    requested_bytes: u64,
    current_bytes: u64,
    first: Sample,
    second: Sample,
    calibrated_fixed_visible_base_bytes: u64,
}

#[derive(Debug, Serialize)]
struct Sample {
    session_id: String,
    sequence: u64,
    observed_unix_millis: u64,
    physical_total_bytes: u64,
    observed_visible_base_bytes: u64,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.len() < 2 {
        return Err("calibration requires VM_NAME ALIAS and explicit options".to_owned());
    }
    validate_identity(&args[0], "VM_NAME")?;
    validate_identity(&args[1], "ALIAS")?;
    let mut telemetry_path = None;
    let mut output = None;
    let mut sample_interval = None;
    let mut max_age = None;
    let mut future_tolerance = None;
    let mut command_timeout = None;
    let mut connection = "qemu:///system".to_owned();
    let mut connection_seen = false;
    let mut index = 2;
    while index < args.len() {
        let option = args[index].as_str();
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("{option} requires a value"))?;
        match option {
            "--telemetry-path" if telemetry_path.is_none() => telemetry_path = Some(value.clone()),
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            "--sample-interval-seconds" if sample_interval.is_none() => {
                sample_interval = Some(positive_duration(value, option)?)
            }
            "--max-age-seconds" if max_age.is_none() => {
                max_age = Some(positive_duration(value, option)?)
            }
            "--future-tolerance-seconds" if future_tolerance.is_none() => {
                future_tolerance = Some(positive_duration(value, option)?)
            }
            "--command-timeout-seconds" if command_timeout.is_none() => {
                command_timeout = Some(positive_duration(value, option)?)
            }
            "--connect" if !connection_seen && !value.trim().is_empty() => {
                connection_seen = true;
                connection = value.clone();
            }
            _ => return Err(format!("unknown or duplicate calibration option: {option}")),
        }
        index += 1;
    }
    let telemetry_path =
        telemetry_path.ok_or_else(|| "calibration requires --telemetry-path".to_owned())?;
    validate_windows_path(&telemetry_path)?;
    Ok(Command {
        vm: args[0].clone(),
        alias: args[1].clone(),
        telemetry_path,
        output: output.ok_or_else(|| "calibration requires --output".to_owned())?,
        sample_interval: sample_interval
            .ok_or_else(|| "calibration requires --sample-interval-seconds".to_owned())?,
        max_age: max_age.ok_or_else(|| "calibration requires --max-age-seconds".to_owned())?,
        future_tolerance: future_tolerance
            .ok_or_else(|| "calibration requires --future-tolerance-seconds".to_owned())?,
        command_timeout: command_timeout
            .ok_or_else(|| "calibration requires --command-timeout-seconds".to_owned())?,
        connection,
    })
}

pub fn run(command: &Command, repo: &Path) -> Result<(), String> {
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
    let geometry = parse_virtio_mem_xml_for_alias(&xml, &command.alias)
        .map_err(|error| format!("parse live virtio-mem geometry: {error}"))?;
    if geometry.memory.requested_bytes != geometry.memory.current_bytes {
        return Err(format!(
            "calibration requires requested == current (requested={}, current={})",
            geometry.memory.requested_bytes, geometry.memory.current_bytes
        ));
    }
    let virsh = Virsh::with_connection("virsh", command.command_timeout, &command.connection);
    let reader = VirshQgaFileReader::new(virsh, command.vm.clone());
    let first = read_sample(command, &reader, geometry.memory.current_bytes)?;
    std::thread::sleep(command.sample_interval);
    let second = read_sample(command, &reader, geometry.memory.current_bytes)?;
    if first.session_id != second.session_id || second.sequence <= first.sequence {
        return Err(
            "calibration requires two advancing samples from one service session".to_owned(),
        );
    }
    if first.observed_visible_base_bytes != second.observed_visible_base_bytes {
        return Err(format!(
            "visible base changed across allocation-neutral samples (first={}, second={})",
            first.observed_visible_base_bytes, second.observed_visible_base_bytes
        ));
    }
    let output = if command.output.is_absolute() {
        command.output.clone()
    } else {
        repo.join(&command.output)
    };
    let captured_unix_millis = now_millis()?;
    persist(
        &output,
        &Evidence {
            schema_version: 1,
            captured_unix_millis,
            vm_name: command.vm.clone(),
            device_alias: command.alias.clone(),
            telemetry_path: command.telemetry_path.clone(),
            block_size_bytes: geometry.memory.block_size_bytes,
            device_size_bytes: geometry.memory.size_bytes,
            requested_bytes: geometry.memory.requested_bytes,
            current_bytes: geometry.memory.current_bytes,
            calibrated_fixed_visible_base_bytes: second.observed_visible_base_bytes,
            first,
            second,
        },
    )?;
    println!("Calibration evidence written to {}", output.display());
    Ok(())
}

fn read_sample(
    command: &Command,
    reader: &impl GuestFileReader,
    current_bytes: u64,
) -> Result<Sample, String> {
    let delivery = reader.read_file(&command.telemetry_path, 64 * 1024)?;
    let envelope: RawTelemetryEnvelope = serde_json::from_slice(&delivery)
        .map_err(|error| format!("parse calibration telemetry: {error}"))?;
    envelope
        .validate_for(
            &command.vm,
            "VirtioMemService",
            now_millis()?,
            u64::try_from(command.max_age.as_millis())
                .map_err(|_| "calibration max age overflows milliseconds".to_owned())?,
            u64::try_from(command.future_tolerance.as_millis())
                .map_err(|_| "calibration future tolerance overflows milliseconds".to_owned())?,
        )
        .map_err(|error| format!("validate calibration telemetry: {error}"))?;
    let observed_visible_base_bytes = envelope
        .memory
        .physical_total_bytes
        .checked_sub(current_bytes)
        .ok_or_else(|| "physical total is below live current allocation".to_owned())?;
    Ok(Sample {
        session_id: envelope.session_id,
        sequence: envelope.sequence,
        observed_unix_millis: envelope.observed_unix_millis,
        physical_total_bytes: envelope.memory.physical_total_bytes,
        observed_visible_base_bytes,
    })
}

fn persist(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "calibration output has no parent".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create calibration evidence directory: {error}"))?;
    let mut json = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("serialize calibration evidence: {error}"))?;
    json.push(b'\n');
    let temporary = parent.join(format!(".calibration-{}.tmp", std::process::id()));
    std::fs::write(&temporary, json)
        .map_err(|error| format!("write calibration evidence: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("commit calibration evidence: {error}"))
}

fn positive_duration(value: &str, option: &str) -> Result<Duration, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .map(Duration::from_secs)
        .ok_or_else(|| format!("{option} requires a positive integer"))
}

fn validate_identity(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty()
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        Err(format!("{name} contains unsafe characters"))
    } else {
        Ok(())
    }
}

fn validate_windows_path(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || value.contains("..")
        || value.chars().any(char::is_control)
    {
        Err("telemetry path must be an absolute Windows drive path without '..'".to_owned())
    } else {
        Ok(())
    }
}

fn now_millis() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_millis(),
    )
    .map_err(|_| "Unix timestamp does not fit in u64 milliseconds".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn requires_explicit_bounded_calibration_scope() {
        let parsed = parse(&strings(&[
            "guest",
            "alias",
            "--telemetry-path",
            r"C:\ProgramData\telemetry.json",
            "--output",
            "evidence.json",
            "--sample-interval-seconds",
            "5",
            "--max-age-seconds",
            "30",
            "--future-tolerance-seconds",
            "5",
            "--command-timeout-seconds",
            "10",
        ]));
        assert!(parsed.is_ok());
        assert!(parse(&strings(&["guest", "alias"])).is_err());
    }
}
