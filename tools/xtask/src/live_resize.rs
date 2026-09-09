use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use virtio_mem_core::{bytes_to_kibibytes, parse_virtio_mem_xml_for_alias, VirtioMemState};

use crate::process;

const DEFAULT_MAX_TARGET_BYTES: u64 = 8 << 30;
const DEFAULT_HOST_RESERVE_BYTES: u64 = 4 << 30;
const MINIMUM_RETAINED_BYTES: u64 = 1 << 30;
const VIRSH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    vm: String,
    alias: String,
    target_bytes: u64,
    apply: bool,
    keep_target: bool,
    timeout_seconds: u64,
    rollback_timeout_seconds: u64,
    interval_seconds: u64,
    connect: String,
    max_target_bytes: u64,
    host_reserve_bytes: u64,
    log_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    state: VirtioMemState,
}

pub fn parse(args: &[String]) -> Result<Options, String> {
    if args.len() < 3 {
        return Err("live-resize requires VM_NAME ALIAS TARGET_BYTES".to_owned());
    }
    let vm = args[0].clone();
    let alias = args[1].clone();
    if !valid_scope(&vm) {
        return Err("VM_NAME must be non-empty and must not begin with '-'".to_owned());
    }
    if alias.is_empty()
        || !alias
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        return Err("ALIAS may contain only letters, digits, '_', '.', and '-'".to_owned());
    }
    let target_bytes = positive(&args[2], "TARGET_BYTES")?;
    let mut options = Options {
        vm,
        alias,
        target_bytes,
        apply: false,
        keep_target: false,
        timeout_seconds: 30,
        rollback_timeout_seconds: 300,
        interval_seconds: 5,
        connect: "qemu:///system".to_owned(),
        max_target_bytes: DEFAULT_MAX_TARGET_BYTES,
        host_reserve_bytes: DEFAULT_HOST_RESERVE_BYTES,
        log_path: None,
    };
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => options.apply = true,
            "--keep-target" => options.keep_target = true,
            "--timeout" => {
                options.timeout_seconds = positive_value(args, &mut index, "--timeout")?;
                if options.timeout_seconds > 30 {
                    return Err("--timeout may not exceed 30 seconds".to_owned());
                }
            }
            "--rollback-timeout" => {
                options.rollback_timeout_seconds =
                    positive_value(args, &mut index, "--rollback-timeout")?;
            }
            "--interval" => {
                options.interval_seconds = positive_value(args, &mut index, "--interval")?;
            }
            "--connect" => options.connect = string_value(args, &mut index, "--connect")?,
            "--max-target-bytes" => {
                options.max_target_bytes = positive_value(args, &mut index, "--max-target-bytes")?;
            }
            "--host-reserve-bytes" => {
                options.host_reserve_bytes =
                    positive_value(args, &mut index, "--host-reserve-bytes")?;
            }
            "--log" => {
                options.log_path = Some(PathBuf::from(string_value(args, &mut index, "--log")?));
            }
            option => return Err(format!("unknown live-resize option: {option}")),
        }
        index += 1;
    }
    if options.keep_target && !options.apply {
        return Err("--keep-target requires --apply".to_owned());
    }
    if !valid_scope(&options.connect) {
        return Err("--connect must be non-empty and contain no control characters".to_owned());
    }
    Ok(options)
}

pub fn run(options: &Options, repo: &Path) -> Result<(), String> {
    if !process::command_exists("virsh") {
        return Err("missing required host command: virsh".to_owned());
    }
    if options.target_bytes < MINIMUM_RETAINED_BYTES {
        return Err("BLOCKED: target must retain at least 1 GiB".to_owned());
    }
    prepare_log(options.log_path.as_deref())?;
    let stop = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&stop))
        .and_then(|_| flag::register(SIGINT, Arc::clone(&stop)))
        .map_err(|error| format!("failed to install cancellation handlers: {error}"))?;

    let initial = read_sample(options, repo)?;
    validate_target(options, initial.state)?;
    let host_available = host_available_bytes(Path::new("/proc/meminfo"))?;
    if options.target_bytes > initial.state.current_bytes {
        let delta = options.target_bytes - initial.state.current_bytes;
        if delta > host_available || host_available - delta < options.host_reserve_bytes {
            return Err(format!(
                "BLOCKED: target needs {delta} additional bytes but host MemAvailable is {host_available} with {} reserved",
                options.host_reserve_bytes
            ));
        }
    }
    println!(
        "baseline vm={} alias={} requested={} current={} size={} block={} target={} max_target={} host_available={} host_reserve={}",
        options.vm,
        options.alias,
        initial.state.requested_bytes,
        initial.state.current_bytes,
        initial.state.size_bytes,
        initial.state.block_size_bytes,
        options.target_bytes,
        options.max_target_bytes,
        host_available,
        options.host_reserve_bytes
    );
    if options.target_bytes == initial.state.current_bytes {
        println!("NO CHANGE: target equals current memory.");
        return Ok(());
    }
    if !options.apply {
        println!("DRY RUN: target is valid, but --apply was not supplied.");
        return Ok(());
    }

    let target_kib = bytes_to_kibibytes(options.target_bytes)
        .ok_or_else(|| "target must be an exact integer number of KiB".to_owned())?;
    println!(
        "APPLY: requesting target {} bytes (forward timeout {}s).",
        options.target_bytes, options.timeout_seconds
    );
    update(options, repo, target_kib)?;

    let forward_result = wait_for_target(
        options,
        repo,
        options.target_bytes,
        options.timeout_seconds,
        &stop,
    );
    if forward_result.is_ok() {
        println!("CONVERGED: target {} bytes reached.", options.target_bytes);
    }

    let rollback_result = if options.keep_target {
        println!("KEEP: target retained by explicit --keep-target.");
        Ok(())
    } else {
        let rollback_target = initial.state.current_bytes.max(MINIMUM_RETAINED_BYTES);
        println!("ROLLBACK: requesting original size {rollback_target} bytes.");
        let rollback_kib = bytes_to_kibibytes(rollback_target)
            .ok_or_else(|| "rollback target must be an exact integer number of KiB".to_owned())?;
        update(options, repo, rollback_kib).and_then(|()| {
            wait_for_target(
                options,
                repo,
                rollback_target,
                options.rollback_timeout_seconds,
                &AtomicBool::new(false),
            )
        })
    };

    if let Err(error) = rollback_result {
        return Err(format!("CRITICAL: rollback did not converge: {error}"));
    }
    if !options.keep_target {
        println!("RESTORED: original size confirmed.");
    }
    forward_result
}

fn validate_target(options: &Options, state: VirtioMemState) -> Result<(), String> {
    state
        .validate_target(options.target_bytes)
        .map_err(|error| format!("BLOCKED: invalid target: {error}"))?;
    if options.target_bytes > options.max_target_bytes {
        return Err(format!(
            "BLOCKED: target {} exceeds safety cap {} bytes",
            options.target_bytes, options.max_target_bytes
        ));
    }
    if state.requested_bytes != state.current_bytes {
        return Err(format!(
            "BLOCKED: existing request has not converged (requested={} current={})",
            state.requested_bytes, state.current_bytes
        ));
    }
    Ok(())
}

fn wait_for_target(
    options: &Options,
    repo: &Path,
    expected: u64,
    timeout_seconds: u64,
    stop: &AtomicBool,
) -> Result<(), String> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(timeout_seconds))
        .ok_or_else(|| "convergence timeout is too large".to_owned())?;
    let mut samples = 0_u64;
    loop {
        let sample = read_sample(options, repo)?;
        samples += 1;
        if sample.state.requested_bytes == expected && sample.state.current_bytes == expected {
            return Ok(());
        }
        if stop.load(Ordering::Relaxed) {
            return Err("operation cancelled".to_owned());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "expected requested=current={expected} within {timeout_seconds} seconds after {samples} samples"
            ));
        }
        std::thread::sleep(Duration::from_secs(options.interval_seconds));
    }
}

fn read_sample(options: &Options, repo: &Path) -> Result<Sample, String> {
    let xml = virsh(options, repo, &["dumpxml", "--live", &options.vm])?;
    let snapshot = parse_virtio_mem_xml_for_alias(&xml, &options.alias)
        .map_err(|error| format!("failed to parse live virtio-mem state: {error}"))?;
    let domstate = virsh(options, repo, &["domstate", &options.vm])?;
    let (qga_free, qga_total) = qga_memory(options, repo).unwrap_or((None, None));
    append_sample(
        options.log_path.as_deref(),
        domstate.trim(),
        snapshot.memory,
        qga_free,
        qga_total,
    )?;
    Ok(Sample {
        state: snapshot.memory,
    })
}

fn update(options: &Options, repo: &Path, requested_kib: u64) -> Result<(), String> {
    virsh(
        options,
        repo,
        &[
            "update-memory-device",
            &options.vm,
            "--alias",
            &options.alias,
            "--requested-size",
            &requested_kib.to_string(),
            "--live",
        ],
    )?;
    Ok(())
}

fn virsh(options: &Options, repo: &Path, command: &[&str]) -> Result<String, String> {
    let mut args = vec![OsString::from("-c"), OsString::from(&options.connect)];
    args.extend(command.iter().map(OsString::from));
    process::bounded_text("virsh", &args, repo, VIRSH_TIMEOUT)
}

fn qga_memory(options: &Options, repo: &Path) -> Result<(Option<u64>, Option<u64>), String> {
    let output = virsh(
        options,
        repo,
        &[
            "qemu-agent-command",
            &options.vm,
            r#"{"execute":"guest-get-memory-stats"}"#,
        ],
    )?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|error| format!("invalid QGA memory JSON: {error}"))?;
    let values = value
        .get("return")
        .and_then(Value::as_array)
        .ok_or_else(|| "QGA memory return must be an array".to_owned())?;
    let stat = |name: &str| {
        values.iter().find_map(|entry| {
            (entry.get("stat").and_then(Value::as_str) == Some(name))
                .then(|| entry.get("value").and_then(Value::as_u64))
                .flatten()
        })
    };
    Ok((stat("stat-free"), stat("stat-total")))
}

fn prepare_log(path: Option<&Path>) -> Result<(), String> {
    if let Some(path) = path {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("failed to open sample log {}: {error}", path.display()))?;
        writeln!(
            file,
            "unix_seconds,domstate,requested_bytes,current_bytes,size_bytes,block_bytes,qga_free_bytes,qga_total_bytes"
        )
        .map_err(|error| format!("failed to write sample log {}: {error}", path.display()))?;
    }
    Ok(())
}

fn append_sample(
    path: Option<&Path>,
    domstate: &str,
    state: VirtioMemState,
    qga_free: Option<u64>,
    qga_total: Option<u64>,
) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_secs();
    let clean_state = domstate.replace([',', '\r', '\n'], " ");
    let display =
        |value: Option<u64>| value.map_or_else(|| "-".to_owned(), |value| value.to_string());
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open sample log {}: {error}", path.display()))?;
    writeln!(
        file,
        "{timestamp},{clean_state},{},{},{},{},{},{}",
        state.requested_bytes,
        state.current_bytes,
        state.size_bytes,
        state.block_size_bytes,
        display(qga_free),
        display(qga_total)
    )
    .map_err(|error| format!("failed to write sample log {}: {error}", path.display()))
}

fn host_available_bytes(path: &Path) -> Result<u64, String> {
    let input = process::read_file(path)?;
    let kib = input.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some("MemAvailable:"))
            .then(|| fields.next()?.parse::<u64>().ok())
            .flatten()
    });
    kib.and_then(|value| value.checked_mul(1024))
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("unable to read MemAvailable from {}", path.display()))
}

fn valid_scope(value: &str) -> bool {
    !value.is_empty() && !value.starts_with('-') && !value.chars().any(char::is_control)
}

fn positive(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} requires a positive decimal integer"))
}

fn positive_value(args: &[String], index: &mut usize, option: &str) -> Result<u64, String> {
    positive(&string_value(args, index, option)?, option)
}

fn string_value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| format!("{option} requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn defaults_to_dry_run_and_bounded_rollback() {
        let options = parse(&strings(&["guest", "memory0", "1073741824"]))
            .expect("valid live-resize arguments");
        assert!(!options.apply);
        assert!(!options.keep_target);
        assert_eq!(options.timeout_seconds, 30);
        assert_eq!(options.rollback_timeout_seconds, 300);
    }

    #[test]
    fn rejects_unsafe_options() {
        assert!(parse(&strings(&["-guest", "memory0", "1073741824"])).is_err());
        assert!(parse(&strings(&["guest", "bad alias", "1073741824"])).is_err());
        assert!(parse(&strings(&["guest", "memory0", "0"])).is_err());
        assert!(parse(&strings(&[
            "guest",
            "memory0",
            "1073741824",
            "--timeout",
            "31"
        ]))
        .is_err());
        assert!(parse(&strings(&[
            "guest",
            "memory0",
            "1073741824",
            "--keep-target"
        ]))
        .is_err());
    }

    #[test]
    fn reuses_shared_target_validation() {
        let options =
            parse(&strings(&["guest", "memory0", "6442450944"])).expect("valid arguments");
        let state = VirtioMemState {
            size_bytes: 8 << 30,
            block_size_bytes: 2 << 20,
            requested_bytes: 4 << 30,
            current_bytes: 4 << 30,
        };
        assert!(validate_target(&options, state).is_ok());
        assert!(validate_target(
            &Options {
                target_bytes: 8 << 30,
                ..options
            },
            state
        )
        .is_err());
    }

    #[test]
    fn parses_mem_available_with_checked_units() {
        let task_path =
            std::env::temp_dir().join(format!("virtio-mem-xtask-meminfo-{}", std::process::id()));
        std::fs::write(&task_path, "MemTotal: 10 kB\nMemAvailable: 4096 kB\n")
            .expect("write fixture");
        let result = host_available_bytes(&task_path);
        std::fs::remove_file(&task_path).expect("remove fixture");
        assert_eq!(result, Ok(4 * 1024 * 1024));
    }
}
