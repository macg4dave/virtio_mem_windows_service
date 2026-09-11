use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use wait_timeout::ChildExt;

use crate::process;

const MAX_INSPECTED_FILES: usize = 32;
const MAX_TEXT_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryCommand {
    instance: String,
    unit: String,
    binary: PathBuf,
    text_files: Vec<PathBuf>,
    output: PathBuf,
    command_timeout: Duration,
    elevate: bool,
    elevated_child: bool,
}

#[derive(Debug, Serialize)]
struct InventoryEvidence {
    schema_version: u32,
    captured_unix_millis: u64,
    instance: String,
    unit: String,
    service_properties: BTreeMap<String, String>,
    binary: FileEvidence,
    text_files: Vec<FileEvidence>,
    controller_active: bool,
}

#[derive(Debug, Serialize)]
struct FileEvidence {
    path: String,
    canonical_path: String,
    sha256: String,
    size_bytes: u64,
    mode: String,
    uid: u32,
    gid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

pub fn parse(arguments: &[String]) -> Result<InventoryCommand, String> {
    if arguments.first().map(String::as_str) != Some("inventory") {
        return Err("deployment requires: inventory INSTANCE [OPTIONS]".to_owned());
    }
    let instance = arguments
        .get(1)
        .ok_or_else(|| "deployment inventory requires INSTANCE".to_owned())?
        .clone();
    validate_identity(&instance, "INSTANCE")?;

    let mut unit = None;
    let mut binary = None;
    let mut text_files = Vec::new();
    let mut output = None;
    let mut command_timeout = None;
    let mut elevate = false;
    let mut elevated_child = false;
    let mut index = 2;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--unit" => unit = unique_value(arguments, &mut index, "--unit", unit)?,
            "--binary" => {
                binary = unique_path(arguments, &mut index, "--binary", binary)?;
            }
            "--text-file" => {
                text_files.push(PathBuf::from(value(arguments, &mut index, "--text-file")?));
            }
            "--output" => output = unique_path(arguments, &mut index, "--output", output)?,
            "--command-timeout-seconds" => {
                if command_timeout.is_some() {
                    return Err("--command-timeout-seconds may be supplied only once".to_owned());
                }
                command_timeout = Some(Duration::from_secs(positive_value(
                    arguments,
                    &mut index,
                    "--command-timeout-seconds",
                )?));
            }
            "--elevate" if !elevate => elevate = true,
            "--elevated-child" if !elevated_child => elevated_child = true,
            "--elevate" => return Err("--elevate may be supplied only once".to_owned()),
            "--elevated-child" => {
                return Err("--elevated-child may be supplied only once".to_owned())
            }
            option => return Err(format!("unknown deployment inventory option: {option}")),
        }
        index += 1;
    }

    if text_files.is_empty() {
        return Err("deployment inventory requires at least one --text-file".to_owned());
    }
    if text_files.len() > MAX_INSPECTED_FILES {
        return Err(format!(
            "deployment inventory accepts at most {MAX_INSPECTED_FILES} text files"
        ));
    }
    if elevate && elevated_child {
        return Err("--elevate and --elevated-child are mutually exclusive".to_owned());
    }
    let unit = unit.ok_or_else(|| "deployment inventory requires --unit UNIT".to_owned())?;
    let expected_unit = format!("virtio-mem-host@{instance}.service");
    if unit != expected_unit {
        return Err(format!(
            "--unit must be the exact instance unit {expected_unit}"
        ));
    }
    let binary = binary.ok_or_else(|| "deployment inventory requires --binary PATH".to_owned())?;
    let output = output.ok_or_else(|| "deployment inventory requires --output PATH".to_owned())?;
    for path in std::iter::once(&binary).chain(text_files.iter()) {
        validate_absolute_path(path)?;
    }
    let mut unique_paths = HashSet::new();
    for path in std::iter::once(&binary).chain(text_files.iter()) {
        if !unique_paths.insert(path.clone()) {
            return Err(format!(
                "deployment inventory path is duplicated: {}",
                path.display()
            ));
        }
    }

    Ok(InventoryCommand {
        instance,
        unit,
        binary,
        text_files,
        output,
        command_timeout: command_timeout.ok_or_else(|| {
            "deployment inventory requires --command-timeout-seconds N".to_owned()
        })?,
        elevate,
        elevated_child,
    })
}

pub fn run(command: &InventoryCommand, repo: &Path) -> Result<(), String> {
    if command.elevate {
        let json = run_elevated(command, repo)?;
        persist_output(&resolve_output(repo, &command.output), json.as_bytes())?;
        println!(
            "Deployment inventory written to {}",
            resolve_output(repo, &command.output).display()
        );
        return Ok(());
    }
    if command.elevated_child {
        require_root(repo, command.command_timeout)?;
        print!("{}", collect(command, repo)?);
        return Ok(());
    }
    let json = collect(command, repo)?;
    persist_output(&resolve_output(repo, &command.output), json.as_bytes())?;
    println!(
        "Deployment inventory written to {}",
        resolve_output(repo, &command.output).display()
    );
    Ok(())
}

fn collect(command: &InventoryCommand, repo: &Path) -> Result<String, String> {
    let properties = systemd_properties(command, repo)?;
    let active_state = properties
        .get("ActiveState")
        .ok_or_else(|| "systemctl did not report ActiveState".to_owned())?;
    if active_state != "inactive" {
        return Err(format!(
            "refusing inventory because {} is {active_state}, expected inactive",
            command.unit
        ));
    }
    let binary = inspect_file(&command.binary, false, command.command_timeout, repo)?;
    let text_files = command
        .text_files
        .iter()
        .map(|path| inspect_file(path, true, command.command_timeout, repo))
        .collect::<Result<Vec<_>, _>>()?;
    let captured_unix_millis = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_millis(),
    )
    .map_err(|_| "Unix timestamp does not fit in u64 milliseconds".to_owned())?;
    serde_json::to_string_pretty(&InventoryEvidence {
        schema_version: 1,
        captured_unix_millis,
        instance: command.instance.clone(),
        unit: command.unit.clone(),
        service_properties: properties,
        binary,
        text_files,
        controller_active: false,
    })
    .map(|mut json| {
        json.push('\n');
        json
    })
    .map_err(|error| format!("encode deployment inventory: {error}"))
}

fn systemd_properties(
    command: &InventoryCommand,
    repo: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let property_list = "Id,Names,LoadState,ActiveState,SubState,UnitFileState,User,Group,MainPID,NRestarts,FragmentPath,DropInPaths,EnvironmentFiles,ExecStart";
    let text = process::bounded_text(
        "/usr/bin/systemctl",
        &[
            process::os("show"),
            process::os(&command.unit),
            process::os(format!("--property={property_list}")),
            process::os("--no-pager"),
        ],
        repo,
        command.command_timeout,
    )?;
    let mut properties = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| format!("systemctl returned malformed property: {line}"))?;
        if properties
            .insert(name.to_owned(), value.to_owned())
            .is_some()
        {
            return Err(format!("systemctl returned duplicate property: {name}"));
        }
    }
    Ok(properties)
}

fn inspect_file(
    path: &Path,
    capture_text: bool,
    timeout: Duration,
    repo: &Path,
) -> Result<FileEvidence, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("inspect deployment file {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "deployment path is not a regular file: {}",
            path.display()
        ));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| format!("canonicalize deployment file {}: {error}", path.display()))?;
    let hash_output = process::bounded_text(
        "/usr/bin/sha256sum",
        &[process::os("--"), path.as_os_str().to_owned()],
        repo,
        timeout,
    )?;
    let sha256 = hash_output
        .split_whitespace()
        .next()
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            format!(
                "sha256sum returned an invalid digest for {}",
                path.display()
            )
        })?
        .to_ascii_lowercase();
    let content =
        if capture_text {
            if metadata.len() > MAX_TEXT_FILE_BYTES {
                return Err(format!(
                    "deployment text file {} exceeds {MAX_TEXT_FILE_BYTES} byte limit",
                    path.display()
                ));
            }
            Some(std::fs::read_to_string(path).map_err(|error| {
                format!("read deployment text file {}: {error}", path.display())
            })?)
        } else {
            None
        };
    Ok(FileEvidence {
        path: path.display().to_string(),
        canonical_path: canonical.display().to_string(),
        sha256,
        size_bytes: metadata.len(),
        mode: format!("{:04o}", metadata.permissions().mode() & 0o7777),
        uid: metadata.uid(),
        gid: metadata.gid(),
        content,
    })
}

fn run_elevated(command: &InventoryCommand, repo: &Path) -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("resolve current xtask executable: {error}"))?;
    let mut child = ProcessCommand::new("sudo")
        .arg("--")
        .arg(executable)
        .args(elevated_arguments(command))
        .current_dir(repo)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("start sudo for deployment inventory: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "capture elevated deployment inventory output".to_owned())?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut stdout = stdout;
        let mut bytes = Vec::new();
        stdout
            .read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|error| format!("read elevated deployment inventory: {error}"))
    });
    let status = if let Some(status) = child
        .wait_timeout(command.command_timeout)
        .map_err(|error| format!("wait for elevated deployment inventory: {error}"))?
    {
        status
    } else {
        child
            .kill()
            .map_err(|error| format!("terminate timed-out elevated inventory: {error}"))?;
        let _ = child.wait();
        let _ = reader.join();
        return Err(format!(
            "elevated deployment inventory timed out after {:?}",
            command.command_timeout
        ));
    };
    let bytes = reader
        .join()
        .map_err(|_| "elevated deployment inventory reader panicked".to_owned())??;
    if !status.success() {
        return Err(format!(
            "elevated deployment inventory failed with status {status}"
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("elevated deployment inventory returned invalid UTF-8: {error}"))
}

fn elevated_arguments(command: &InventoryCommand) -> Vec<OsString> {
    let mut arguments = vec![
        process::os("deployment"),
        process::os("inventory"),
        process::os(&command.instance),
        process::os("--unit"),
        process::os(&command.unit),
        process::os("--binary"),
        command.binary.as_os_str().to_owned(),
    ];
    for path in &command.text_files {
        arguments.push(process::os("--text-file"));
        arguments.push(path.as_os_str().to_owned());
    }
    arguments.extend([
        process::os("--output"),
        command.output.as_os_str().to_owned(),
        process::os("--command-timeout-seconds"),
        process::os(command.command_timeout.as_secs().to_string()),
        process::os("--elevated-child"),
    ]);
    arguments
}

fn require_root(repo: &Path, timeout: Duration) -> Result<(), String> {
    let uid = process::bounded_text("/usr/bin/id", &[process::os("-u")], repo, timeout)?;
    if uid.trim() == "0" {
        Ok(())
    } else {
        Err("--elevated-child requires effective uid 0".to_owned())
    }
}

fn persist_output(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "deployment inventory output requires a parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create deployment inventory output directory: {error}"))?;
    let temporary = PathBuf::from(format!("{}.tmp-{}", path.display(), std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("create deployment inventory temporary file: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("write deployment inventory: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("flush deployment inventory: {error}"))?;
    drop(file);
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("publish deployment inventory: {error}")
    })?;
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("flush deployment inventory directory: {error}"))
}

fn resolve_output(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

fn validate_identity(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Err(format!("{name} contains unsupported characters"))
    } else {
        Ok(())
    }
}

fn validate_absolute_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        Err(format!(
            "deployment inventory path must be absolute without '..': {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}

fn value(arguments: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    arguments
        .get(*index)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn positive_value(arguments: &[String], index: &mut usize, option: &str) -> Result<u64, String> {
    let value = value(arguments, index, option)?;
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{option} requires a positive integer"))
}

fn unique_value(
    arguments: &[String],
    index: &mut usize,
    option: &str,
    current: Option<String>,
) -> Result<Option<String>, String> {
    if current.is_some() {
        return Err(format!("{option} may be supplied only once"));
    }
    Ok(Some(value(arguments, index, option)?))
}

fn unique_path(
    arguments: &[String],
    index: &mut usize,
    option: &str,
    current: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    unique_value(
        arguments,
        index,
        option,
        current.map(|path| path.display().to_string()),
    )
    .map(|value| value.map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_arguments() -> Vec<String> {
        [
            "inventory",
            "guest",
            "--unit",
            "virtio-mem-host@guest.service",
            "--binary",
            "/usr/local/libexec/virtio-mem-host",
            "--text-file",
            "/etc/systemd/system/virtio-mem-host@.service",
            "--text-file",
            "/etc/virtio-mem-host/guest.conf",
            "--output",
            ".artifacts/deployment/guest.json",
            "--command-timeout-seconds",
            "10",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn parses_explicit_inventory_scope() {
        let command = parse(&valid_arguments()).expect("valid inventory");
        assert_eq!(command.instance, "guest");
        assert_eq!(command.text_files.len(), 2);
        assert_eq!(command.command_timeout, Duration::from_secs(10));
        assert!(!command.elevate);
    }

    #[test]
    fn rejects_implicit_or_ambiguous_scope() {
        let mut missing = valid_arguments();
        missing.drain(4..6);
        assert!(parse(&missing).unwrap_err().contains("--binary"));

        let mut wrong_unit = valid_arguments();
        wrong_unit[3] = "virtio-mem-host@other.service".to_owned();
        assert!(parse(&wrong_unit)
            .unwrap_err()
            .contains("exact instance unit"));

        let mut duplicate = valid_arguments();
        duplicate.splice(
            8..8,
            [
                "--text-file".to_owned(),
                "/etc/virtio-mem-host/guest.conf".to_owned(),
            ],
        );
        assert!(parse(&duplicate).unwrap_err().contains("duplicated"));
    }

    #[test]
    fn elevated_child_preserves_every_reviewed_argument() {
        let mut arguments = valid_arguments();
        arguments.push("--elevate".to_owned());
        let command = parse(&arguments).expect("elevated inventory");
        let elevated = elevated_arguments(&command);
        assert_eq!(elevated.first(), Some(&OsString::from("deployment")));
        assert_eq!(elevated.last(), Some(&OsString::from("--elevated-child")));
        assert_eq!(
            elevated
                .iter()
                .filter(|value| value.as_os_str() == "--text-file")
                .count(),
            2
        );
    }

    #[test]
    fn output_is_written_atomically_by_the_unprivileged_parent() {
        let directory = std::env::temp_dir().join(format!(
            "virtio-mem-deployment-inventory-{}",
            std::process::id()
        ));
        let output = directory.join("inventory.json");
        let _ = std::fs::remove_dir_all(&directory);
        persist_output(&output, b"{\"schema_version\":1}\n").expect("persist evidence");
        assert_eq!(
            std::fs::read_to_string(&output).expect("read evidence"),
            "{\"schema_version\":1}\n"
        );
        std::fs::remove_dir_all(directory).expect("remove evidence fixture");
    }
}
