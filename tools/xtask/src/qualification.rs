use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use virtio_mem_core::parse_virtio_mem_xml_for_alias;

use crate::process;

const SCHEMA_VERSION: u32 = 1;
const DEFAULT_PEAK_BYTES: u64 = 4 << 30;
const DEFAULT_RETAINED_BYTES: u64 = 2 << 30;
const DEFAULT_GROWTH_BYTES: u64 = 1 << 30;
const DEFAULT_RECLAIM_BYTES: u64 = 64 << 20;
const MAX_HOLD_SECONDS: u64 = 3_600;
const MAX_RUN_SECONDS: u64 = 4 * 3_600;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WorkloadMode {
    Committed,
    Resident,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Config {
    version: u32,
    run_id: String,
    vm_name: String,
    device_alias: String,
    ssh_target: String,
    connect_uri: String,
    controller_unit: String,
    remote_workload: String,
    telemetry_path: Option<PathBuf>,
    mode: WorkloadMode,
    peak_bytes: u64,
    retained_bytes: u64,
    peak_hold_seconds: u64,
    settled_hold_seconds: u64,
    renewed_hold_seconds: u64,
    post_hold_seconds: u64,
    interval_seconds: u64,
    expect_growth_bytes: u64,
    expect_reclaim_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Status {
    version: u32,
    run_id: String,
    state: String,
    pid: Option<u32>,
    started_unix_millis: Option<u128>,
    finished_unix_millis: Option<u128>,
    final_result: Option<String>,
    message: String,
}

#[derive(Debug)]
struct StartOptions {
    config: Config,
    output_root: PathBuf,
    apply: bool,
}

#[derive(Debug, Default)]
struct Observations {
    initial_current: Option<u64>,
    maximum_current: u64,
    minimum_settled_current: Option<u64>,
    workload_phases: Vec<String>,
    sample_count: u64,
    warnings: u64,
}

pub fn execute(arguments: &[String], repo: &Path) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("start") => start(parse_start(&arguments[1..], repo)?, repo),
        Some("run") => run_internal(&arguments[1..], repo),
        Some("status") => show_status(&arguments[1..], repo),
        Some("review") => review(&arguments[1..], repo),
        _ => Err("qualification requires start, status, or review".to_owned()),
    }
}

fn parse_start(args: &[String], repo: &Path) -> Result<StartOptions, String> {
    if args.len() < 2 {
        return Err("qualification start requires VM_NAME ALIAS and --ssh-target TARGET".to_owned());
    }
    let vm_name = scope(&args[0], "VM_NAME")?;
    let device_alias = identifier(&args[1], "ALIAS")?;
    let mut ssh_target = None;
    let mut mode = WorkloadMode::Resident;
    let mut peak_bytes = DEFAULT_PEAK_BYTES;
    let mut retained_bytes = DEFAULT_RETAINED_BYTES;
    let mut peak_hold_seconds = 600;
    let mut settled_hold_seconds = 900;
    let mut renewed_hold_seconds = 600;
    let mut post_hold_seconds = 60;
    let mut interval_seconds = 5;
    let mut expect_growth_bytes = DEFAULT_GROWTH_BYTES;
    let mut expect_reclaim_bytes = DEFAULT_RECLAIM_BYTES;
    let mut remote_workload = r"C:\Users\Public\virtio-mem-build\target\release\virtio-mem-workload.exe".to_owned();
    let mut controller_unit = format!("virtio-mem-host@{vm_name}.service");
    let mut telemetry_path = None;
    let mut connect_uri = "qemu:///system".to_owned();
    let mut output_root = repo.join(".vscode-artifacts/qualification");
    let mut apply = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => apply = true,
            "--ssh-target" => ssh_target = Some(value(args, &mut index, "--ssh-target")?),
            "--profile" => {
                mode = match value(args, &mut index, "--profile")?.as_str() {
                    "m10g-resident" => WorkloadMode::Resident,
                    "m10g-committed" => WorkloadMode::Committed,
                    _ => return Err("--profile must be m10g-resident or m10g-committed".to_owned()),
                }
            }
            "--peak-bytes" => peak_bytes = number(args, &mut index, "--peak-bytes")?,
            "--retained-bytes" => retained_bytes = number(args, &mut index, "--retained-bytes")?,
            "--peak-hold-seconds" => peak_hold_seconds = number(args, &mut index, "--peak-hold-seconds")?,
            "--settled-hold-seconds" => settled_hold_seconds = number(args, &mut index, "--settled-hold-seconds")?,
            "--renewed-hold-seconds" => renewed_hold_seconds = number(args, &mut index, "--renewed-hold-seconds")?,
            "--post-hold-seconds" => post_hold_seconds = number(args, &mut index, "--post-hold-seconds")?,
            "--interval-seconds" => interval_seconds = number(args, &mut index, "--interval-seconds")?,
            "--expect-growth-bytes" => expect_growth_bytes = number(args, &mut index, "--expect-growth-bytes")?,
            "--expect-reclaim-bytes" => expect_reclaim_bytes = number(args, &mut index, "--expect-reclaim-bytes")?,
            "--remote-workload" => remote_workload = value(args, &mut index, "--remote-workload")?,
            "--controller-unit" => controller_unit = value(args, &mut index, "--controller-unit")?,
            "--telemetry-path" => telemetry_path = Some(PathBuf::from(value(args, &mut index, "--telemetry-path")?)),
            "--connect" => connect_uri = value(args, &mut index, "--connect")?,
            "--output-root" => output_root = absolute_or_repo(repo, &value(args, &mut index, "--output-root")?),
            option => return Err(format!("unknown qualification option: {option}")),
        }
        index += 1;
    }
    let ssh_target = scope(&ssh_target.ok_or_else(|| "--ssh-target is required; refusing to infer a guest endpoint".to_owned())?, "--ssh-target")?;
    if remote_workload.is_empty() || remote_workload.chars().any(char::is_control) || remote_workload.chars().any(|c| "&|<>^%!\"".contains(c)) {
        return Err("--remote-workload contains characters unsafe for cmd.exe".to_owned());
    }
    scope(&connect_uri, "--connect")?;
    identifier(&controller_unit, "--controller-unit")?;
    if peak_bytes == 0 || retained_bytes == 0 || retained_bytes >= peak_bytes || peak_bytes > 8 << 30 {
        return Err("workload bytes require 0 < retained < peak <= 8 GiB".to_owned());
    }
    for (name, seconds) in [("peak", peak_hold_seconds), ("settled", settled_hold_seconds), ("renewed", renewed_hold_seconds)] {
        if seconds == 0 || seconds > MAX_HOLD_SECONDS { return Err(format!("{name} hold must be 1..={MAX_HOLD_SECONDS} seconds")); }
    }
    if interval_seconds == 0 || post_hold_seconds > MAX_HOLD_SECONDS || peak_hold_seconds + settled_hold_seconds + renewed_hold_seconds + post_hold_seconds > MAX_RUN_SECONDS {
        return Err("sampling/post-run duration is zero or total run exceeds four hours".to_owned());
    }
    let run_id = new_run_id()?;
    Ok(StartOptions { config: Config { version: SCHEMA_VERSION, run_id, vm_name, device_alias, ssh_target, connect_uri, controller_unit, remote_workload, telemetry_path, mode, peak_bytes, retained_bytes, peak_hold_seconds, settled_hold_seconds, renewed_hold_seconds, post_hold_seconds, interval_seconds, expect_growth_bytes, expect_reclaim_bytes }, output_root, apply })
}

fn start(options: StartOptions, repo: &Path) -> Result<(), String> {
    if !options.apply {
        println!("DRY RUN: qualification configuration is valid; add --apply to start it.\n{}", serde_json::to_string_pretty(&options.config).map_err(|e| e.to_string())?);
        return Ok(());
    }
    if !process::command_exists("setsid") { return Err("setsid is required for an unattended qualification run".to_owned()); }
    let run_dir = options.output_root.join(&options.config.run_id);
    std::fs::create_dir_all(&run_dir).map_err(|e| format!("create {}: {e}", run_dir.display()))?;
    write_json(&run_dir.join("config.json"), &options.config)?;
    let initial_status = Status { version: SCHEMA_VERSION, run_id: options.config.run_id.clone(), state: "starting".to_owned(), pid: None, started_unix_millis: None, finished_unix_millis: None, final_result: None, message: "detached supervisor is starting".to_owned() };
    write_json(&run_dir.join("status.json"), &initial_status)?;
    let log = OpenOptions::new().create(true).append(true).open(run_dir.join("supervisor.log")).map_err(|e| format!("open supervisor log: {e}"))?;
    let error_log = log.try_clone().map_err(|e| format!("clone supervisor log: {e}"))?;
    let executable = std::env::current_exe().map_err(|e| format!("locate xtask executable: {e}"))?;
    let child = ProcessCommand::new("setsid").arg(executable).args(["qualification", "run", "--run-dir"]).arg(&run_dir).current_dir(repo).stdin(Stdio::null()).stdout(Stdio::from(log)).stderr(Stdio::from(error_log)).spawn().map_err(|e| format!("start detached supervisor: {e}"))?;
    let mut status = initial_status;
    status.pid = Some(child.id());
    status.message = "detached supervisor launched".to_owned();
    write_json(&run_dir.join("status.json"), &status)?;
    println!("qualification run started\nrun_id={}\noutput_dir={}\npid={}", options.config.run_id, run_dir.display(), child.id());
    Ok(())
}

fn run_internal(args: &[String], repo: &Path) -> Result<(), String> {
    if args.len() != 2 || args[0] != "--run-dir" { return Err("qualification run is an internal command requiring --run-dir PATH".to_owned()); }
    let run_dir = PathBuf::from(&args[1]);
    let config: Config = serde_json::from_str(&process::read_file(&run_dir.join("config.json"))?).map_err(|e| format!("parse qualification config: {e}"))?;
    let started = now_millis()?;
    let mut status = Status { version: SCHEMA_VERSION, run_id: config.run_id.clone(), state: "running".to_owned(), pid: Some(std::process::id()), started_unix_millis: Some(started), finished_unix_millis: None, final_result: None, message: "preflight".to_owned() };
    write_json(&run_dir.join("status.json"), &status)?;
    event(&run_dir, "info", "run_started", json!({"configuration": config}))?;
    let result = supervise(&config, &run_dir, repo);
    let finished = now_millis()?;
    status.state = "finished".to_owned();
    status.finished_unix_millis = Some(finished);
    status.final_result = Some(if result.is_ok() { "pass" } else { "fail" }.to_owned());
    status.message = result.as_ref().map_or_else(|e| e.clone(), |_| "qualification criteria passed".to_owned());
    write_json(&run_dir.join("status.json"), &status)?;
    let summary = json!({"version": SCHEMA_VERSION, "run_id": config.run_id, "test": "automatic_controller_workload_resize", "started_unix_millis": started, "finished_unix_millis": finished, "duration_millis": finished.saturating_sub(started), "status": status.final_result, "message": status.message, "artifacts": ["config.json", "status.json", "events.jsonl", "host-metrics.jsonl", "workload.jsonl", "controller.log", "supervisor.log"]});
    write_json(&run_dir.join("summary.json"), &summary)?;
    event(&run_dir, if result.is_ok() { "info" } else { "failure" }, "run_finished", summary)?;
    result
}

fn supervise(config: &Config, run_dir: &Path, repo: &Path) -> Result<(), String> {
    for command in ["ssh", "virsh"] { if !process::command_exists(command) { return Err(format!("missing prerequisite: {command}")); } }
    let mut observations = Observations::default();
    sample_host(config, run_dir, repo, "baseline", &mut observations)?;
    let remote = workload_command(config);
    event(run_dir, "info", "workload_start", json!({"ssh_target": config.ssh_target, "command": remote}))?;
    let mut child = ProcessCommand::new("ssh").args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=4", "--", &config.ssh_target, &remote]).current_dir(repo).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("start remote workload: {e}"))?;
    let stdout = child.stdout.take().ok_or_else(|| "capture workload stdout".to_owned())?;
    let stderr = child.stderr.take().ok_or_else(|| "capture workload stderr".to_owned())?;
    let (sender, receiver) = mpsc::channel();
    spawn_reader(stdout, false, sender.clone());
    spawn_reader(stderr, true, sender);
    let started = Instant::now();
    let max_duration = Duration::from_secs(config.peak_hold_seconds + config.settled_hold_seconds + config.renewed_hold_seconds + config.post_hold_seconds + 120);
    let interval = Duration::from_secs(config.interval_seconds);
    let mut next_sample = Instant::now() + interval;
    let mut phase = "baseline".to_owned();
    let exit_status;
    loop {
        while let Ok((is_error, line)) = receiver.try_recv() {
            if is_error {
                observations.warnings += 1;
                event(run_dir, "warning", "workload_stderr", json!({"message": line}))?;
            } else {
                append_line(&run_dir.join("workload.jsonl"), &line)?;
                let parsed: Value = serde_json::from_str(&line).map_err(|e| format!("workload emitted invalid JSON: {e}: {line}"))?;
                phase = parsed.get("phase").and_then(Value::as_str).ok_or_else(|| "workload record has no phase".to_owned())?.to_owned();
                observations.workload_phases.push(phase.clone());
                event(run_dir, "info", "workload_phase", parsed)?;
            }
        }
        if let Some(status) = child.try_wait().map_err(|e| format!("poll workload: {e}"))? { exit_status = status; break; }
        if started.elapsed() > max_duration { let _ = child.kill(); return Err("workload exceeded its bounded deadline".to_owned()); }
        if Instant::now() >= next_sample {
            if let Err(error) = sample_host(config, run_dir, repo, &phase, &mut observations) {
                observations.warnings += 1;
                event(run_dir, "warning", "host_sample_failed", json!({"error": error}))?;
            }
            next_sample = Instant::now() + interval;
        }
        thread::sleep(Duration::from_millis(200));
    }
    while let Ok((is_error, line)) = receiver.recv_timeout(Duration::from_millis(100)) {
        if is_error { observations.warnings += 1; event(run_dir, "warning", "workload_stderr", json!({"message": line}))?; }
        else { append_line(&run_dir.join("workload.jsonl"), &line)?; if let Ok(parsed) = serde_json::from_str::<Value>(&line) { if let Some(value) = parsed.get("phase").and_then(Value::as_str) { observations.workload_phases.push(value.to_owned()); } event(run_dir, "info", "workload_phase", parsed)?; } }
    }
    if !exit_status.success() { return Err(format!("remote workload exited with {exit_status}")); }
    let post_deadline = Instant::now() + Duration::from_secs(config.post_hold_seconds);
    while Instant::now() < post_deadline {
        sample_host(config, run_dir, repo, "post_workload", &mut observations)?;
        thread::sleep(interval.min(post_deadline.saturating_duration_since(Instant::now())));
    }
    archive_controller_log(config, run_dir, repo);
    classify(config, &observations)
}

fn sample_host(config: &Config, run_dir: &Path, repo: &Path, phase: &str, observations: &mut Observations) -> Result<(), String> {
    let xml = virsh(config, repo, &["dumpxml", "--live", &config.vm_name])?;
    let memory = parse_virtio_mem_xml_for_alias(&xml, &config.device_alias).map_err(|e| format!("parse virtio-mem state: {e}"))?.memory;
    let domstate = virsh(config, repo, &["domstate", &config.vm_name])?;
    if !domstate.to_ascii_lowercase().contains("running") { return Err(format!("VM is not running: {}", domstate.trim())); }
    let dommemstat = virsh(config, repo, &["dommemstat", &config.vm_name]).unwrap_or_else(|e| format!("error {e}"));
    let host = process::read_file(Path::new("/proc/meminfo"))?;
    let host_available_bytes = meminfo_value(&host, "MemAvailable:");
    let guest_stats = whitespace_pairs(&dommemstat);
    let telemetry = config.telemetry_path.as_deref().and_then(last_complete_line).and_then(|line| serde_json::from_str::<Value>(&line).ok());
    let sample = json!({"version": SCHEMA_VERSION, "run_id": config.run_id, "unix_millis": now_millis()?, "workload_phase": phase, "vm_state": domstate.trim(), "requested_bytes": memory.requested_bytes, "current_bytes": memory.current_bytes, "device_size_bytes": memory.size_bytes, "block_size_bytes": memory.block_size_bytes, "host_mem_available_bytes": host_available_bytes, "guest_dommemstat_kib": guest_stats, "windows_raw_telemetry": telemetry});
    append_json(&run_dir.join("host-metrics.jsonl"), &sample)?;
    observations.initial_current.get_or_insert(memory.current_bytes);
    observations.maximum_current = observations.maximum_current.max(memory.current_bytes);
    if matches!(phase, "settled" | "post_workload" | "complete") {
        observations.minimum_settled_current = Some(observations.minimum_settled_current.map_or(memory.current_bytes, |value| value.min(memory.current_bytes)));
    }
    observations.sample_count += 1;
    Ok(())
}

fn classify(config: &Config, observations: &Observations) -> Result<(), String> {
    let required = ["baseline", "peak", "settled", "renewed", "complete"];
    for phase in required { if !observations.workload_phases.iter().any(|value| value == phase) { return Err(format!("missing workload phase: {phase}")); } }
    let initial = observations.initial_current.ok_or_else(|| "no initial memory sample".to_owned())?;
    let growth = observations.maximum_current.saturating_sub(initial);
    let reclaimed = observations.minimum_settled_current.map_or(0, |minimum| observations.maximum_current.saturating_sub(minimum));
    if growth < config.expect_growth_bytes { return Err(format!("observed growth {growth} bytes is below required {}", config.expect_growth_bytes)); }
    if reclaimed < config.expect_reclaim_bytes { return Err(format!("observed reclaim {reclaimed} bytes is below required {}", config.expect_reclaim_bytes)); }
    Ok(())
}

fn workload_command(config: &Config) -> String {
    let mode = match config.mode { WorkloadMode::Committed => "committed", WorkloadMode::Resident => "resident" };
    format!("\"{}\" --workload-id {} --mode {mode} --peak-bytes {} --retained-bytes {} --peak-hold-seconds {} --settled-hold-seconds {} --renewed-hold-seconds {}", config.remote_workload, config.run_id, config.peak_bytes, config.retained_bytes, config.peak_hold_seconds, config.settled_hold_seconds, config.renewed_hold_seconds)
}

fn archive_controller_log(config: &Config, run_dir: &Path, repo: &Path) {
    let since = run_dir.join("status.json");
    let started = process::read_file(&since).ok().and_then(|s| serde_json::from_str::<Status>(&s).ok()).and_then(|s| s.started_unix_millis).map(|v| format!("@{}", v / 1000)).unwrap_or_else(|| "-1 hour".to_owned());
    let args = [OsString::from("--no-pager"), OsString::from("--output=short-iso"), OsString::from("--unit"), OsString::from(&config.controller_unit), OsString::from("--since"), OsString::from(since)];
    match process::checked_output("journalctl", &args, repo) { Ok(bytes) => { let _ = std::fs::write(run_dir.join("controller.log"), bytes); }, Err(error) => { let _ = event(run_dir, "warning", "controller_log_failed", json!({"error": error})); } }
}

fn show_status(args: &[String], repo: &Path) -> Result<(), String> {
    let run_dir = locate_run(args, repo)?;
    let status = process::read_file(&run_dir.join("status.json"))?;
    println!("{status}\noutput_dir={}", run_dir.display());
    Ok(())
}

fn review(args: &[String], repo: &Path) -> Result<(), String> {
    let run_dir = locate_run(args, repo)?;
    let summary = process::read_file(&run_dir.join("summary.json")).unwrap_or_else(|_| process::read_file(&run_dir.join("status.json")).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")));
    println!("{summary}\noutput_dir={}", run_dir.display());
    for name in ["events.jsonl", "host-metrics.jsonl", "workload.jsonl", "controller.log", "supervisor.log"] { println!("artifact={}", run_dir.join(name).display()); }
    Ok(())
}

fn locate_run(args: &[String], repo: &Path) -> Result<PathBuf, String> {
    if args.is_empty() { return Err("RUN_ID is required".to_owned()); }
    let run_id = identifier(&args[0], "RUN_ID")?;
    let mut root = repo.join(".vscode-artifacts/qualification");
    if args.len() == 3 && args[1] == "--output-root" { root = absolute_or_repo(repo, &args[2]); } else if args.len() != 1 { return Err("expected RUN_ID [--output-root PATH]".to_owned()); }
    Ok(root.join(run_id))
}

fn virsh(config: &Config, repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut values = vec![OsString::from("-c"), OsString::from(&config.connect_uri)];
    values.extend(args.iter().map(OsString::from));
    process::bounded_text("virsh", &values, repo, COMMAND_TIMEOUT)
}

fn spawn_reader(input: impl std::io::Read + Send + 'static, is_error: bool, sender: mpsc::Sender<(bool, String)>) {
    thread::spawn(move || for line in BufReader::new(input).lines().map_while(Result::ok) { let _ = sender.send((is_error, line)); });
}

fn event(run_dir: &Path, level: &str, kind: &str, detail: Value) -> Result<(), String> {
    append_json(&run_dir.join("events.jsonl"), &json!({"version": SCHEMA_VERSION, "unix_millis": now_millis()?, "level": level, "event": kind, "detail": detail}))
}

fn append_json(path: &Path, value: &Value) -> Result<(), String> { append_line(path, &serde_json::to_string(value).map_err(|e| format!("serialize evidence: {e}"))?) }
fn append_line(path: &Path, line: &str) -> Result<(), String> { let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|e| format!("open {}: {e}", path.display()))?; writeln!(file, "{line}").and_then(|_| file.flush()).map_err(|e| format!("write {}: {e}", path.display())) }
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> { let temporary = path.with_extension("tmp"); let mut file = File::create(&temporary).map_err(|e| format!("create {}: {e}", temporary.display()))?; serde_json::to_writer_pretty(&mut file, value).map_err(|e| format!("encode {}: {e}", path.display()))?; file.write_all(b"\n").and_then(|_| file.sync_all()).map_err(|e| format!("flush {}: {e}", path.display()))?; std::fs::rename(&temporary, path).map_err(|e| format!("publish {}: {e}", path.display())) }
fn now_millis() -> Result<u128, String> { SystemTime::now().duration_since(UNIX_EPOCH).map(|v| v.as_millis()).map_err(|e| format!("system clock before Unix epoch: {e}")) }
fn new_run_id() -> Result<String, String> { Ok(format!("qualification-{}-{}", now_millis()?, std::process::id())) }
fn value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> { *index += 1; args.get(*index).filter(|v| !v.is_empty()).cloned().ok_or_else(|| format!("{option} requires a value")) }
fn number(args: &[String], index: &mut usize, option: &str) -> Result<u64, String> { value(args, index, option)?.parse().map_err(|_| format!("{option} requires an unsigned integer")) }
fn scope(value: &str, name: &str) -> Result<String, String> { if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) { Err(format!("{name} is invalid")) } else { Ok(value.to_owned()) } }
fn identifier(value: &str, name: &str) -> Result<String, String> { if value.is_empty() || !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'@')) { Err(format!("{name} contains unsafe characters")) } else { Ok(value.to_owned()) } }
fn absolute_or_repo(repo: &Path, value: &str) -> PathBuf { let path = PathBuf::from(value); if path.is_absolute() { path } else { repo.join(path) } }
fn meminfo_value(input: &str, name: &str) -> Option<u64> { input.lines().find_map(|line| { let mut fields = line.split_whitespace(); (fields.next() == Some(name)).then(|| fields.next()?.parse::<u64>().ok()?.checked_mul(1024)).flatten() }) }
fn whitespace_pairs(input: &str) -> serde_json::Map<String, Value> { input.lines().filter_map(|line| { let mut parts = line.split_whitespace(); Some((parts.next()?.to_owned(), Value::from(parts.next()?.parse::<u64>().ok()?))) }).collect() }
fn last_complete_line(path: &Path) -> Option<String> { let value = std::fs::read_to_string(path).ok()?; if !value.ends_with('\n') { return None; } value.lines().next_back().map(str::to_owned) }

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> { values.iter().map(|v| (*v).to_owned()).collect() }

    #[test]
    fn canonical_profile_is_bounded_and_requires_apply() {
        let repo = Path::new("/repo");
        let parsed = parse_start(&strings(&["vm", "memory0", "--ssh-target", "guest", "--profile", "m10g-resident"]), repo).expect("profile");
        assert!(!parsed.apply);
        assert_eq!(parsed.config.mode, WorkloadMode::Resident);
        assert_eq!(parsed.config.peak_bytes, 4 << 30);
        assert_eq!(parsed.config.expect_reclaim_bytes, 64 << 20);
    }

    #[test]
    fn rejects_implicit_endpoint_and_unbounded_workload() {
        assert!(parse_start(&strings(&["vm", "memory0"]), Path::new("/repo")).is_err());
        assert!(parse_start(&strings(&["vm", "memory0", "--ssh-target", "guest", "--peak-hold-seconds", "3601"]), Path::new("/repo")).is_err());
    }

    #[test]
    fn result_requires_all_phases_and_resize_thresholds() {
        let mut options = parse_start(&strings(&["vm", "memory0", "--ssh-target", "guest"]), Path::new("/repo")).expect("options");
        options.config.expect_growth_bytes = 100;
        options.config.expect_reclaim_bytes = 50;
        let observations = Observations { initial_current: Some(1_000), maximum_current: 1_200, minimum_settled_current: Some(1_100), workload_phases: ["baseline", "peak", "settled", "renewed", "complete"].iter().map(|v| (*v).to_owned()).collect(), sample_count: 5, warnings: 0 };
        assert!(classify(&options.config, &observations).is_ok());
        options.config.expect_growth_bytes = 201;
        assert!(classify(&options.config, &observations).is_err());
    }

    #[test]
    fn parses_host_memory_and_dommemstat_without_guessing_units() {
        assert_eq!(meminfo_value("MemAvailable: 1024 kB\n", "MemAvailable:"), Some(1 << 20));
        let values = whitespace_pairs("unused 10\navailable 20\nlast-update 30\n");
        assert_eq!(values["available"], 20);
    }
}
