use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use virtio_mem_host::attestation::{AttestedCompatibilitySource, CompatibilityAttestation};
use virtio_mem_host::compatibility_source::CompatibilitySource;
use virtio_mem_host::config::HostConfig;
use virtio_mem_host::virsh::Virsh;
use wait_timeout::ChildExt;

use crate::process;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    instance: String,
    config: PathBuf,
    attestation: PathBuf,
    output: PathBuf,
    timeout: Duration,
    apply: bool,
    elevate: bool,
    elevated_child: bool,
}

#[derive(Debug, Serialize)]
struct Evidence {
    schema_version: u32,
    captured_unix_millis: u64,
    applied: bool,
    instance: String,
    unit: String,
    candidate_binary_sha256: String,
    installed_binary_sha256: Option<String>,
    config_sha256: String,
    attestation_sha256: String,
    unit_sha256: String,
    active_state: String,
    unit_file_state: String,
    restart_policy: String,
    environment_files: Vec<String>,
    backup_path: Option<String>,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let instance = args
        .first()
        .ok_or_else(|| "host-deploy requires INSTANCE and explicit options".to_owned())?;
    validate_identity(instance)?;
    let mut config = None;
    let mut attestation = None;
    let mut output = None;
    let mut timeout = None;
    let mut apply = false;
    let mut elevate = false;
    let mut elevated_child = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" if !apply => apply = true,
            "--elevate" if !elevate => elevate = true,
            "--elevated-child" if !elevated_child => elevated_child = true,
            option @ ("--config" | "--attestation" | "--output" | "--command-timeout-seconds") => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option {
                    "--config" if config.is_none() => config = Some(PathBuf::from(value)),
                    "--attestation" if attestation.is_none() => {
                        attestation = Some(PathBuf::from(value))
                    }
                    "--output" if output.is_none() => output = Some(PathBuf::from(value)),
                    "--command-timeout-seconds" if timeout.is_none() => {
                        timeout = Some(Duration::from_secs(
                            value
                                .parse::<u64>()
                                .ok()
                                .filter(|v| *v > 0)
                                .ok_or_else(|| format!("{option} requires a positive integer"))?,
                        ));
                    }
                    _ => return Err(format!("duplicate host-deploy option: {option}")),
                }
            }
            option => return Err(format!("unknown or duplicate host-deploy option: {option}")),
        }
        index += 1;
    }
    if elevate && elevated_child {
        return Err("--elevate and --elevated-child are mutually exclusive".to_owned());
    }
    if (elevate || elevated_child) && !apply {
        return Err("host-deploy elevation requires --apply".to_owned());
    }
    Ok(Command {
        instance: instance.clone(),
        config: config.ok_or_else(|| "host-deploy requires --config".to_owned())?,
        attestation: attestation.ok_or_else(|| "host-deploy requires --attestation".to_owned())?,
        output: output.ok_or_else(|| "host-deploy requires --output".to_owned())?,
        timeout: timeout
            .ok_or_else(|| "host-deploy requires --command-timeout-seconds".to_owned())?,
        apply,
        elevate,
        elevated_child,
    })
}

pub fn run(command: &Command, repo: &Path) -> Result<(), String> {
    if command.elevate {
        let json = run_elevated(command, repo)?;
        persist(&resolve(repo, &command.output), json.as_bytes())?;
        println!(
            "Host deployment evidence written to {}",
            resolve(repo, &command.output).display()
        );
        return Ok(());
    }
    let evidence = collect(command, repo)?;
    let json = serde_json::to_string_pretty(&evidence)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("serialize host deployment evidence: {error}"))?;
    if command.elevated_child {
        print!("{json}");
    } else {
        persist(&resolve(repo, &command.output), json.as_bytes())?;
        println!(
            "Host deployment dry-run evidence written to {}",
            resolve(repo, &command.output).display()
        );
    }
    Ok(())
}

fn collect(command: &Command, repo: &Path) -> Result<Evidence, String> {
    let config_source = resolve(repo, &command.config);
    let attestation_source = resolve(repo, &command.attestation);
    let candidate = repo.join("target/release/virtio-mem-host");
    let unit_source = repo.join("host/systemd/virtio-mem-host@.service");
    for path in [
        &config_source,
        &attestation_source,
        &candidate,
        &unit_source,
    ] {
        if !path.is_file() {
            return Err(format!(
                "required host deployment input is missing: {}",
                path.display()
            ));
        }
    }
    let values = parse_environment(
        &std::fs::read_to_string(&config_source)
            .map_err(|error| format!("read host config: {error}"))?,
    )?;
    validate_config(&values)?;
    let attestation_text = std::fs::read_to_string(&attestation_source)
        .map_err(|error| format!("read compatibility attestation: {error}"))?;
    CompatibilityAttestation::parse(&attestation_text)?;
    validate_live_attestation(&values, &attestation_source, command.timeout)?;
    let unit = format!("virtio-mem-host@{}.service", command.instance);
    let before = properties(&unit, repo, command.timeout)?;
    require_inactive(&before, &unit)?;

    let candidate_hash = hash(&candidate, repo, command.timeout)?;
    let config_hash = hash(&config_source, repo, command.timeout)?;
    let attestation_hash = hash(&attestation_source, repo, command.timeout)?;
    let unit_hash = hash(&unit_source, repo, command.timeout)?;
    let mut backup_path = None;
    if command.apply {
        if !command.elevated_child || unsafe { libc_geteuid() } != 0 {
            return Err("host-deploy --apply must run through --elevate".to_owned());
        }
        backup_path = Some(install(
            command,
            repo,
            &config_source,
            &attestation_source,
            &candidate,
            &unit_source,
            &before,
        )?);
    }
    let after = properties(&unit, repo, command.timeout)?;
    require_inactive(&after, &unit)?;
    let restart_policy = one(&after, "Restart")?;
    if command.apply && restart_policy != "no" {
        return Err(format!(
            "installed unit is not fail-stop: Restart={restart_policy}"
        ));
    }
    let environment_files = after.get("EnvironmentFiles").cloned().unwrap_or_default();
    Ok(Evidence {
        schema_version: 1,
        captured_unix_millis: now_millis()?,
        applied: command.apply,
        instance: command.instance.clone(),
        unit,
        candidate_binary_sha256: candidate_hash,
        installed_binary_sha256: command
            .apply
            .then(|| {
                hash(
                    Path::new("/usr/local/libexec/virtio-mem-host"),
                    repo,
                    command.timeout,
                )
            })
            .transpose()?,
        config_sha256: config_hash,
        attestation_sha256: attestation_hash,
        unit_sha256: unit_hash,
        active_state: one(&after, "ActiveState")?,
        unit_file_state: one(&after, "UnitFileState")?,
        restart_policy,
        environment_files,
        backup_path,
    })
}

fn install(
    command: &Command,
    repo: &Path,
    config: &Path,
    attestation: &Path,
    candidate: &Path,
    unit: &Path,
    before: &BTreeMap<String, Vec<String>>,
) -> Result<String, String> {
    install_directory(
        "/var/lib/virtio-mem-host",
        "root",
        "root",
        "0755",
        repo,
        command.timeout,
    )?;
    install_directory(
        "/var/lib/virtio-mem-host/deployment-backups",
        "root",
        "root",
        "0700",
        repo,
        command.timeout,
    )?;
    install_directory(
        "/var/lib/virtio-mem-host/state",
        "virtio-mem-host",
        "virtio-mem-host",
        "0700",
        repo,
        command.timeout,
    )?;
    let backup = PathBuf::from(format!(
        "/var/lib/virtio-mem-host/deployment-backups/{}",
        now_millis()?
    ));
    std::fs::create_dir_all(&backup)
        .map_err(|error| format!("create host deployment backup: {error}"))?;
    process::bounded_text(
        "chmod",
        &[process::os("0700"), backup.as_os_str().to_owned()],
        repo,
        command.timeout,
    )?;
    let target_config = PathBuf::from(format!("/etc/virtio-mem-host/{}.conf", command.instance));
    let target_attestation = PathBuf::from(format!(
        "/etc/virtio-mem-host/{}.attestation.json",
        command.instance
    ));
    let targets = [
        PathBuf::from("/usr/local/libexec/virtio-mem-host"),
        PathBuf::from("/etc/systemd/system/virtio-mem-host@.service"),
        target_config.clone(),
        target_attestation.clone(),
    ];
    for target in &targets {
        if target.exists() {
            std::fs::copy(
                target,
                backup.join(target.file_name().expect("target filename")),
            )
            .map_err(|error| format!("backup {}: {error}", target.display()))?;
        }
    }
    let dropin = PathBuf::from(format!(
        "/etc/systemd/system/virtio-mem-host@{}.service.d",
        command.instance
    ));
    if dropin.exists() {
        std::fs::rename(&dropin, backup.join("instance-dropins"))
            .map_err(|error| format!("backup obsolete instance drop-ins: {error}"))?;
    }
    let legacy = PathBuf::from(format!(
        "/etc/virtio-mem-host/{}-memory-quanta.conf",
        command.instance
    ));
    if legacy.exists() {
        std::fs::rename(&legacy, backup.join("legacy-memory-quanta.conf"))
            .map_err(|error| format!("backup obsolete split config: {error}"))?;
    }
    install_file(
        candidate,
        "/usr/local/libexec/virtio-mem-host",
        "root",
        "root",
        "0755",
        repo,
        command.timeout,
    )?;
    install_file(
        unit,
        "/etc/systemd/system/virtio-mem-host@.service",
        "root",
        "root",
        "0644",
        repo,
        command.timeout,
    )?;
    install_file(
        config,
        target_config.to_str().expect("UTF-8 config target"),
        "root",
        "virtio-mem-host",
        "0640",
        repo,
        command.timeout,
    )?;
    install_file(
        attestation,
        target_attestation
            .to_str()
            .expect("UTF-8 attestation target"),
        "root",
        "virtio-mem-host",
        "0640",
        repo,
        command.timeout,
    )?;
    process::bounded_text(
        "systemctl",
        &[process::os("daemon-reload")],
        repo,
        command.timeout,
    )?;
    process::bounded_text(
        "systemctl",
        &[
            process::os("disable"),
            process::os(format!("virtio-mem-host@{}.service", command.instance)),
        ],
        repo,
        command.timeout,
    )?;
    if one(before, "UnitFileState")? == "enabled" {
        std::fs::write(backup.join("was-enabled"), b"true\n")
            .map_err(|error| format!("record prior enablement: {error}"))?;
    }
    Ok(backup.display().to_string())
}

fn install_file(
    source: &Path,
    target: &str,
    owner: &str,
    group: &str,
    mode: &str,
    repo: &Path,
    timeout: Duration,
) -> Result<(), String> {
    process::bounded_text(
        "install",
        &[
            process::os("-o"),
            process::os(owner),
            process::os("-g"),
            process::os(group),
            process::os("-m"),
            process::os(mode),
            process::os("--"),
            source.as_os_str().to_owned(),
            process::os(target),
        ],
        repo,
        timeout,
    )
    .map(|_| ())
}

fn install_directory(
    target: &str,
    owner: &str,
    group: &str,
    mode: &str,
    repo: &Path,
    timeout: Duration,
) -> Result<(), String> {
    process::bounded_text(
        "install",
        &[
            process::os("-d"),
            process::os("-o"),
            process::os(owner),
            process::os("-g"),
            process::os(group),
            process::os("-m"),
            process::os(mode),
            process::os("--"),
            process::os(target),
        ],
        repo,
        timeout,
    )
    .map(|_| ())
}

fn validate_config(values: &BTreeMap<String, String>) -> Result<(), String> {
    for (name, value) in values {
        std::env::set_var(name, value);
    }
    HostConfig::from_env()
        .map(|_| ())
        .map_err(|error| format!("validate host configuration: {error}"))
}

fn validate_live_attestation(
    values: &BTreeMap<String, String>,
    path: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let vm = values
        .get("VIRTIO_MEM_VM_NAME")
        .ok_or_else(|| "host config lacks VM identity".to_owned())?;
    let alias = values
        .get("VIRTIO_MEM_ALIAS")
        .ok_or_else(|| "host config lacks alias identity".to_owned())?;
    AttestedCompatibilitySource::new(
        Virsh::with_connection("virsh", timeout, "qemu:///system"),
        vm,
        alias,
        path,
    )
    .compatibility()?
    .validate_for_resize()
    .map_err(|error| format!("validate live compatibility attestation: {error}"))
}

fn parse_environment(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (name, encoded_value) = line
            .split_once('=')
            .ok_or_else(|| format!("malformed host environment line: {line}"))?;
        let value = parse_environment_value(encoded_value)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(format!("unsafe host environment line: {line}"));
        }
        if values.insert(name.to_owned(), value).is_some() {
            return Err(format!("duplicate host environment variable: {name}"));
        }
    }
    Ok(values)
}

fn parse_environment_value(value: &str) -> Result<String, String> {
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        let decoded = &value[1..value.len() - 1];
        if decoded.is_empty() || decoded.contains('\'') || decoded.chars().any(char::is_control) {
            return Err("unsafe single-quoted host environment value".to_owned());
        }
        return Ok(decoded.to_owned());
    }
    if value.is_empty()
        || value.contains(['\\', '\'', '"'])
        || value.chars().any(char::is_whitespace)
    {
        return Err(
            "host environment values containing backslashes or whitespace must use single quotes"
                .to_owned(),
        );
    }
    Ok(value.to_owned())
}

fn properties(
    unit: &str,
    repo: &Path,
    timeout: Duration,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let text = process::bounded_text("systemctl", &[process::os("show"), process::os(unit), process::os("--property=ActiveState,SubState,UnitFileState,Restart,EnvironmentFiles,DropInPaths"), process::os("--no-pager")], repo, timeout)?;
    let mut result = BTreeMap::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("malformed systemctl property: {line}"))?;
        result
            .entry(key.to_owned())
            .or_insert_with(Vec::new)
            .push(value.to_owned());
    }
    Ok(result)
}

fn one(values: &BTreeMap<String, Vec<String>>, key: &str) -> Result<String, String> {
    let found = values
        .get(key)
        .ok_or_else(|| format!("systemctl omitted {key}"))?;
    if found.len() != 1 {
        return Err(format!("systemctl returned repeated {key}"));
    }
    Ok(found[0].clone())
}

fn require_inactive(values: &BTreeMap<String, Vec<String>>, unit: &str) -> Result<(), String> {
    let state = one(values, "ActiveState")?;
    if state == "inactive" {
        Ok(())
    } else {
        Err(format!(
            "refusing host deployment because {unit} is {state}"
        ))
    }
}

fn hash(path: &Path, repo: &Path, timeout: Duration) -> Result<String, String> {
    process::bounded_text(
        "sha256sum",
        &[process::os("--"), path.as_os_str().to_owned()],
        repo,
        timeout,
    )?
    .split_whitespace()
    .next()
    .filter(|value| value.len() == 64)
    .map(str::to_owned)
    .ok_or_else(|| format!("invalid SHA-256 for {}", path.display()))
}

fn run_elevated(command: &Command, repo: &Path) -> Result<String, String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("resolve xtask executable: {error}"))?;
    let mut child = ProcessCommand::new("sudo")
        .arg("--")
        .arg(executable)
        .args(elevated_arguments(command))
        .current_dir(repo)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("start sudo host deployment: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "capture elevated host deployment".to_owned())?;
    let reader = std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut bytes = Vec::new();
        stdout
            .read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|error| format!("read elevated host deployment: {error}"))
    });
    let workflow_timeout = command
        .timeout
        .checked_mul(16)
        .ok_or_else(|| "elevated host deployment timeout overflowed".to_owned())?;
    let status = if let Some(status) = child
        .wait_timeout(workflow_timeout)
        .map_err(|error| format!("wait for elevated host deployment: {error}"))?
    {
        status
    } else {
        child
            .kill()
            .map_err(|error| format!("terminate timed-out elevated host deployment: {error}"))?;
        let _ = child.wait();
        let _ = reader.join();
        return Err(format!(
            "elevated host deployment timed out after {workflow_timeout:?}"
        ));
    };
    let bytes = reader
        .join()
        .map_err(|_| "elevated host deployment stdout reader panicked".to_owned())??;
    if !status.success() {
        return Err(format!("elevated host deployment failed with {status}"));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("elevated host deployment returned invalid UTF-8: {error}"))
}

fn elevated_arguments(command: &Command) -> Vec<OsString> {
    vec![
        process::os("host-deploy"),
        process::os(&command.instance),
        process::os("--config"),
        command.config.as_os_str().to_owned(),
        process::os("--attestation"),
        command.attestation.as_os_str().to_owned(),
        process::os("--output"),
        command.output.as_os_str().to_owned(),
        process::os("--command-timeout-seconds"),
        process::os(command.timeout.as_secs().to_string()),
        process::os("--apply"),
        process::os("--elevated-child"),
    ]
}

fn persist(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "host deployment output has no parent".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create host deployment evidence directory: {error}"))?;
    let temp = parent.join(format!(".host-deployment-{}.tmp", std::process::id()));
    std::fs::write(&temp, bytes)
        .map_err(|error| format!("write host deployment evidence: {error}"))?;
    std::fs::rename(&temp, path)
        .map_err(|error| format!("commit host deployment evidence: {error}"))
}

fn resolve(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}
fn validate_identity(value: &str) -> Result<(), String> {
    if !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        Ok(())
    } else {
        Err("host-deploy INSTANCE contains unsafe characters".to_owned())
    }
}
fn now_millis() -> Result<u64, String> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_millis(),
    )
    .map_err(|_| "Unix timestamp does not fit u64".to_owned())
}

unsafe extern "C" {
    fn geteuid() -> u32;
}
unsafe fn libc_geteuid() -> u32 {
    unsafe { geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_explicit_apply_and_elevation() {
        let args = [
            "guest",
            "--config",
            "host.conf",
            "--attestation",
            "attestation.json",
            "--output",
            "evidence.json",
            "--command-timeout-seconds",
            "30",
            "--apply",
            "--elevate",
        ]
        .map(str::to_owned);
        assert!(matches!(
            parse(&args),
            Ok(Command {
                apply: true,
                elevate: true,
                ..
            })
        ));
    }
    #[test]
    fn environment_parser_rejects_duplicates_and_shell_syntax() {
        assert!(parse_environment("A=1\nB=two\n").is_ok());
        assert!(parse_environment("A=1\nA=2\n").is_err());
        assert!(parse_environment("export A=1\n").is_err());
        assert!(parse_environment(r"PATH=C:\ProgramData\telemetry.json").is_err());
        assert_eq!(
            parse_environment(r"PATH='C:\ProgramData\telemetry.json'")
                .expect("quoted Windows path")["PATH"],
            r"C:\ProgramData\telemetry.json"
        );
    }
}
