use std::collections::BTreeMap;
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
use virtio_mem_core::{parse_virtio_mem_xml_for_alias, RawTelemetryEnvelope};
use virtio_mem_host::qga::{GuestFileReader, VirshQgaFileReader};
use virtio_mem_host::virsh::Virsh;
use wait_timeout::ChildExt;

use crate::process;

const SCHEMA_VERSION: u32 = 2;
const CONTROLLER_GUARD_ROOT: &str = "/run/virtio-mem-qualification";

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
    guest_service: String,
    remote_workload: String,
    telemetry_path: String,
    telemetry_max_age_seconds: u64,
    telemetry_future_tolerance_seconds: u64,
    mode: WorkloadMode,
    peak_bytes: u64,
    retained_bytes: u64,
    max_allocation_bytes: u64,
    peak_hold_seconds: u64,
    settled_hold_seconds: u64,
    renewed_hold_seconds: u64,
    resident_refresh_seconds: u64,
    post_hold_seconds: u64,
    interval_seconds: u64,
    command_timeout_seconds: u64,
    controller_timeout_seconds: u64,
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

#[derive(Debug, Serialize, Deserialize)]
struct ControllerGuardStatus {
    version: u32,
    run_id: String,
    state: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct LiveStateEvidence {
    version: u32,
    run_id: String,
    stage: String,
    unix_millis: u128,
    vm_state: String,
    controller_active_state: String,
    controller_unit_file_state: String,
    requested_bytes: u64,
    current_bytes: u64,
    device_size_bytes: u64,
    block_size_bytes: u64,
    host_mem_available_bytes: Option<u64>,
}

#[derive(Debug, Default)]
struct Observations {
    initial_current: Option<u64>,
    maximum_current: u64,
    maximum_peak_current: u64,
    minimum_settled_current: Option<u64>,
    workload_phases: Vec<String>,
    sample_count: u64,
    warnings: u64,
    last_requested: Option<u64>,
}

pub fn execute(arguments: &[String], repo: &Path) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("start") => start(parse_start(&arguments[1..], repo)?, repo),
        Some("run") => run_internal(&arguments[1..], repo),
        Some("guard") => guard_internal(&arguments[1..], repo),
        Some("status") => show_status(&arguments[1..], repo),
        Some("review") => review(&arguments[1..], repo),
        _ => Err("qualification requires start, status, or review".to_owned()),
    }
}

fn parse_start(args: &[String], repo: &Path) -> Result<StartOptions, String> {
    if args.len() < 2 {
        return Err(
            "qualification start requires VM_NAME ALIAS and --ssh-target TARGET".to_owned(),
        );
    }
    let vm_name = scope(&args[0], "VM_NAME")?;
    let device_alias = identifier(&args[1], "ALIAS")?;
    let mut ssh_target = None;
    let mut mode = None;
    let mut peak_bytes = None;
    let mut retained_bytes = None;
    let mut max_allocation_bytes = None;
    let mut peak_hold_seconds = None;
    let mut settled_hold_seconds = None;
    let mut renewed_hold_seconds = None;
    let mut resident_refresh_seconds = None;
    let mut post_hold_seconds = None;
    let mut interval_seconds = None;
    let mut command_timeout_seconds = None;
    let mut controller_timeout_seconds = None;
    let mut expect_growth_bytes = None;
    let mut expect_reclaim_bytes = None;
    let mut remote_workload = None;
    let mut controller_unit = None;
    let mut guest_service = None;
    let mut telemetry_path = None;
    let mut telemetry_max_age_seconds = None;
    let mut telemetry_future_tolerance_seconds = None;
    let mut connect_uri = "qemu:///system".to_owned();
    let mut output_root = repo.join(".artifacts/qualification");
    let mut apply = false;
    let mut elevate = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => apply = true,
            "--elevate" => elevate = true,
            "--ssh-target" => ssh_target = Some(value(args, &mut index, "--ssh-target")?),
            "--mode" => {
                mode = Some(match value(args, &mut index, "--mode")?.as_str() {
                    "resident" => WorkloadMode::Resident,
                    "committed" => WorkloadMode::Committed,
                    _ => return Err("--mode must be resident or committed".to_owned()),
                })
            }
            "--peak-bytes" => peak_bytes = Some(number(args, &mut index, "--peak-bytes")?),
            "--retained-bytes" => {
                retained_bytes = Some(number(args, &mut index, "--retained-bytes")?)
            }
            "--max-allocation-bytes" => {
                max_allocation_bytes = Some(number(args, &mut index, "--max-allocation-bytes")?)
            }
            "--peak-hold-seconds" => {
                peak_hold_seconds = Some(number(args, &mut index, "--peak-hold-seconds")?)
            }
            "--settled-hold-seconds" => {
                settled_hold_seconds = Some(number(args, &mut index, "--settled-hold-seconds")?)
            }
            "--renewed-hold-seconds" => {
                renewed_hold_seconds = Some(number(args, &mut index, "--renewed-hold-seconds")?)
            }
            "--resident-refresh-seconds" => {
                resident_refresh_seconds =
                    Some(number(args, &mut index, "--resident-refresh-seconds")?)
            }
            "--post-hold-seconds" => {
                post_hold_seconds = Some(number(args, &mut index, "--post-hold-seconds")?)
            }
            "--interval-seconds" => {
                interval_seconds = Some(number(args, &mut index, "--interval-seconds")?)
            }
            "--command-timeout-seconds" => {
                command_timeout_seconds =
                    Some(number(args, &mut index, "--command-timeout-seconds")?)
            }
            "--controller-timeout-seconds" => {
                controller_timeout_seconds =
                    Some(number(args, &mut index, "--controller-timeout-seconds")?)
            }
            "--expect-growth-bytes" => {
                expect_growth_bytes = Some(number(args, &mut index, "--expect-growth-bytes")?)
            }
            "--expect-reclaim-bytes" => {
                expect_reclaim_bytes = Some(number(args, &mut index, "--expect-reclaim-bytes")?)
            }
            "--remote-workload" => {
                remote_workload = Some(value(args, &mut index, "--remote-workload")?)
            }
            "--controller-unit" => {
                controller_unit = Some(value(args, &mut index, "--controller-unit")?)
            }
            "--guest-service" => guest_service = Some(value(args, &mut index, "--guest-service")?),
            "--telemetry-path" => {
                telemetry_path = Some(value(args, &mut index, "--telemetry-path")?)
            }
            "--telemetry-max-age-seconds" => {
                telemetry_max_age_seconds =
                    Some(number(args, &mut index, "--telemetry-max-age-seconds")?)
            }
            "--telemetry-future-tolerance-seconds" => {
                telemetry_future_tolerance_seconds = Some(number(
                    args,
                    &mut index,
                    "--telemetry-future-tolerance-seconds",
                )?)
            }
            "--connect" => connect_uri = value(args, &mut index, "--connect")?,
            "--output-root" => {
                output_root = absolute_or_repo(repo, &value(args, &mut index, "--output-root")?)
            }
            option => return Err(format!("unknown qualification option: {option}")),
        }
        index += 1;
    }
    let ssh_target = scope(
        &ssh_target.ok_or_else(|| {
            "--ssh-target is required; refusing to infer a guest endpoint".to_owned()
        })?,
        "--ssh-target",
    )?;
    let mode = required(mode, "--mode")?;
    let peak_bytes = required(peak_bytes, "--peak-bytes")?;
    let retained_bytes = required(retained_bytes, "--retained-bytes")?;
    let max_allocation_bytes = required(max_allocation_bytes, "--max-allocation-bytes")?;
    let peak_hold_seconds = required(peak_hold_seconds, "--peak-hold-seconds")?;
    let settled_hold_seconds = required(settled_hold_seconds, "--settled-hold-seconds")?;
    let renewed_hold_seconds = required(renewed_hold_seconds, "--renewed-hold-seconds")?;
    let resident_refresh_seconds =
        required(resident_refresh_seconds, "--resident-refresh-seconds")?;
    let post_hold_seconds = required(post_hold_seconds, "--post-hold-seconds")?;
    let interval_seconds = required(interval_seconds, "--interval-seconds")?;
    let command_timeout_seconds = required(command_timeout_seconds, "--command-timeout-seconds")?;
    let controller_timeout_seconds =
        required(controller_timeout_seconds, "--controller-timeout-seconds")?;
    let expect_growth_bytes = required(expect_growth_bytes, "--expect-growth-bytes")?;
    let expect_reclaim_bytes = required(expect_reclaim_bytes, "--expect-reclaim-bytes")?;
    let remote_workload = required(remote_workload, "--remote-workload")?;
    let controller_unit = required(controller_unit, "--controller-unit")?;
    let guest_service = required(guest_service, "--guest-service")?;
    let telemetry_path = required(telemetry_path, "--telemetry-path")?;
    let telemetry_max_age_seconds =
        required(telemetry_max_age_seconds, "--telemetry-max-age-seconds")?;
    let telemetry_future_tolerance_seconds = required(
        telemetry_future_tolerance_seconds,
        "--telemetry-future-tolerance-seconds",
    )?;
    if remote_workload.is_empty()
        || remote_workload.chars().any(char::is_control)
        || remote_workload.chars().any(|c| "&|<>^%!\"".contains(c))
    {
        return Err("--remote-workload contains characters unsafe for cmd.exe".to_owned());
    }
    scope(&connect_uri, "--connect")?;
    identifier(&controller_unit, "--controller-unit")?;
    identifier(&guest_service, "--guest-service")?;
    let expected_controller_unit = format!("virtio-mem-host@{vm_name}.service");
    if controller_unit != expected_controller_unit {
        return Err(format!(
            "--controller-unit must be {expected_controller_unit} for the selected VM"
        ));
    }
    crate::calibration::validate_windows_path(&telemetry_path)?;
    if max_allocation_bytes == 0
        || peak_bytes == 0
        || peak_bytes > max_allocation_bytes
        || retained_bytes == 0
        || retained_bytes >= peak_bytes
    {
        return Err("workload bytes require 0 < retained < peak <= max allocation".to_owned());
    }
    for (name, seconds) in [
        ("peak", peak_hold_seconds),
        ("settled", settled_hold_seconds),
        ("renewed", renewed_hold_seconds),
    ] {
        if seconds == 0 {
            return Err(format!("{name} hold must be positive"));
        }
    }
    if resident_refresh_seconds == 0
        || interval_seconds == 0
        || command_timeout_seconds == 0
        || controller_timeout_seconds == 0
        || telemetry_max_age_seconds == 0
    {
        return Err("refresh, sampling, and command timeout values must be positive".to_owned());
    }
    if expect_growth_bytes == 0 || expect_reclaim_bytes == 0 {
        return Err("resize expectations must be positive".to_owned());
    }
    let minimum_controller_seconds = peak_hold_seconds
        .checked_add(settled_hold_seconds)
        .and_then(|value| value.checked_add(renewed_hold_seconds))
        .and_then(|value| value.checked_add(post_hold_seconds))
        .and_then(|value| value.checked_add(command_timeout_seconds.checked_mul(10)?))
        .ok_or_else(|| "configured qualification duration overflows".to_owned())?;
    if controller_timeout_seconds < minimum_controller_seconds {
        return Err(format!(
            "--controller-timeout-seconds must be at least {minimum_controller_seconds} for the configured run"
        ));
    }
    if apply && !elevate {
        return Err(
            "qualification --apply requires --elevate for bounded controller ownership".to_owned(),
        );
    }
    if elevate && !apply {
        return Err("qualification --elevate requires --apply".to_owned());
    }
    let run_id = new_run_id()?;
    Ok(StartOptions {
        config: Config {
            version: SCHEMA_VERSION,
            run_id,
            vm_name,
            device_alias,
            ssh_target,
            connect_uri,
            controller_unit,
            guest_service,
            remote_workload,
            telemetry_path,
            telemetry_max_age_seconds,
            telemetry_future_tolerance_seconds,
            mode,
            peak_bytes,
            retained_bytes,
            max_allocation_bytes,
            peak_hold_seconds,
            settled_hold_seconds,
            renewed_hold_seconds,
            resident_refresh_seconds,
            post_hold_seconds,
            interval_seconds,
            command_timeout_seconds,
            controller_timeout_seconds,
            expect_growth_bytes,
            expect_reclaim_bytes,
        },
        output_root,
        apply,
    })
}

fn start(options: StartOptions, repo: &Path) -> Result<(), String> {
    if !options.apply {
        println!(
            "DRY RUN: qualification configuration is valid; add --apply to start it.\n{}",
            serde_json::to_string_pretty(&options.config).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if !process::command_exists("setsid") {
        return Err("setsid is required for an unattended qualification run".to_owned());
    }
    let run_dir = options.output_root.join(&options.config.run_id);
    std::fs::create_dir_all(&run_dir).map_err(|e| format!("create {}: {e}", run_dir.display()))?;
    write_json(&run_dir.join("config.json"), &options.config)?;
    let initial_status = Status {
        version: SCHEMA_VERSION,
        run_id: options.config.run_id.clone(),
        state: "starting".to_owned(),
        pid: None,
        started_unix_millis: None,
        finished_unix_millis: None,
        final_result: None,
        message: "detached supervisor is starting".to_owned(),
    };
    write_json(&run_dir.join("status.json"), &initial_status)?;
    let preparation = (|| {
        let initial_state = capture_live_state(&options.config, repo, "before_controller")?;
        if initial_state.requested_bytes != initial_state.current_bytes {
            return Err(format!(
                "qualification requires converged initial state; requested={} current={}",
                initial_state.requested_bytes, initial_state.current_bytes
            ));
        }
        write_json(&run_dir.join("initial-state.json"), &initial_state)?;
        preflight_guest_health(&options.config, &run_dir, repo, "before_controller")
    })();
    if let Err(error) = preparation {
        finish_start_failure(&run_dir, initial_status, &error)?;
        return Err(error);
    }
    if let Err(error) =
        launch_controller_guard(&run_dir, repo, options.config.command_timeout_seconds)
    {
        finish_start_failure(&run_dir, initial_status, &error)?;
        return Err(error);
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run_dir.join("supervisor.log"))
        .map_err(|e| format!("open supervisor log: {e}"))?;
    let error_log = log
        .try_clone()
        .map_err(|e| format!("clone supervisor log: {e}"))?;
    let executable =
        std::env::current_exe().map_err(|e| format!("locate xtask executable: {e}"))?;
    let child = match ProcessCommand::new("setsid")
        .arg(executable)
        .args(["qualification", "run", "--run-dir"])
        .arg(&run_dir)
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(error_log))
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let launch_error = format!("start detached supervisor: {error}");
            let release = write_release(&run_dir);
            let cleanup = wait_for_guard_stopped(
                &run_dir,
                &options.config,
                Duration::from_secs(options.config.command_timeout_seconds)
                    .checked_mul(3)
                    .ok_or_else(|| "controller cleanup timeout overflowed".to_owned())?,
                Duration::from_secs(options.config.interval_seconds),
            );
            let combined =
                format!("{launch_error}; controller release={release:?}; cleanup={cleanup:?}");
            finish_start_failure(&run_dir, initial_status, &combined)?;
            return Err(combined);
        }
    };
    let mut status = initial_status;
    status.pid = Some(child.id());
    status.message = "detached supervisor launched".to_owned();
    write_json(&run_dir.join("status.json"), &status)?;
    println!(
        "qualification run started\nrun_id={}\noutput_dir={}\npid={}",
        options.config.run_id,
        run_dir.display(),
        child.id()
    );
    Ok(())
}

fn launch_controller_guard(
    run_dir: &Path,
    repo: &Path,
    timeout_seconds: u64,
) -> Result<(), String> {
    let executable =
        std::env::current_exe().map_err(|error| format!("locate xtask executable: {error}"))?;
    let mut child = ProcessCommand::new("sudo")
        .arg("--")
        .arg(executable)
        .args(["qualification", "guard", "--run-dir"])
        .arg(run_dir)
        .arg("--launch")
        .current_dir(repo)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("start elevated qualification controller guard: {error}"))?;
    let timeout = Duration::from_secs(timeout_seconds)
        .checked_mul(4)
        .ok_or_else(|| "controller guard launch timeout overflowed".to_owned())?;
    let status = child
        .wait_timeout(timeout)
        .map_err(|error| format!("wait for elevated qualification controller guard: {error}"))?
        .ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            format!("elevated qualification controller guard timed out after {timeout:?}")
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "elevated qualification controller guard failed with {status}"
        ))
    }
}

fn guard_internal(args: &[String], repo: &Path) -> Result<(), String> {
    if args.len() != 3
        || args[0] != "--run-dir"
        || !matches!(args[2].as_str(), "--launch" | "--serve")
    {
        return Err(
            "qualification guard is internal and requires --run-dir PATH --launch|--serve"
                .to_owned(),
        );
    }
    if unsafe { libc_geteuid() } != 0 {
        return Err("qualification controller guard requires effective uid 0".to_owned());
    }
    let run_dir = PathBuf::from(&args[1]);
    let config: Config = serde_json::from_str(&process::read_file(&run_dir.join("config.json"))?)
        .map_err(|error| format!("parse qualification config: {error}"))?;
    if run_dir.file_name().and_then(|value| value.to_str()) != Some(config.run_id.as_str()) {
        return Err("qualification run directory does not match the configured run ID".to_owned());
    }
    if args[2] == "--serve" {
        return serve_controller_guard(&config, &run_dir, repo);
    }
    launch_controller_guard_daemon(&config, &run_dir, repo)
}

fn launch_controller_guard_daemon(
    config: &Config,
    run_dir: &Path,
    repo: &Path,
) -> Result<(), String> {
    let properties = systemd_properties(
        &config.controller_unit,
        repo,
        config.command_timeout_seconds,
    )?;
    if properties.get("ActiveState").map(String::as_str) != Some("inactive")
        || properties.get("UnitFileState").map(String::as_str) != Some("disabled")
    {
        return Err(format!(
            "qualification requires disabled/inactive {}; got UnitFileState={} ActiveState={}",
            config.controller_unit,
            properties
                .get("UnitFileState")
                .map_or("missing", String::as_str),
            properties
                .get("ActiveState")
                .map_or("missing", String::as_str)
        ));
    }
    let live = capture_live_state(config, repo, "guard_revalidation")?;
    if live.requested_bytes != live.current_bytes {
        return Err(format!(
            "qualification guard found divergent live state; requested={} current={}",
            live.requested_bytes, live.current_bytes
        ));
    }
    write_root_guard_status(
        run_dir,
        config,
        "starting",
        "starting the exclusive controller",
    )?;
    if let Err(error) = systemctl_action(
        "start",
        &config.controller_unit,
        repo,
        config.command_timeout_seconds,
    )
    .and_then(|_| require_controller_active_and_shrink(config, repo))
    {
        let cleanup = systemctl_action(
            "stop",
            &config.controller_unit,
            repo,
            config.command_timeout_seconds,
        );
        write_root_guard_status(
            run_dir,
            config,
            "failed",
            &format!("{error}; cleanup={cleanup:?}"),
        )?;
        return Err(error);
    }
    let executable =
        std::env::current_exe().map_err(|error| format!("locate xtask executable: {error}"))?;
    if let Err(error) = ProcessCommand::new("setsid")
        .arg(executable)
        .args(["qualification", "guard", "--run-dir"])
        .arg(run_dir)
        .arg("--serve")
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        let _ = systemctl_action(
            "stop",
            &config.controller_unit,
            repo,
            config.command_timeout_seconds,
        );
        return Err(format!("start detached controller guard: {error}"));
    }
    write_root_guard_status(
        run_dir,
        config,
        "running",
        "exclusive controller is active under the bounded guard",
    )
}

fn serve_controller_guard(config: &Config, run_dir: &Path, repo: &Path) -> Result<(), String> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(config.controller_timeout_seconds))
        .ok_or_else(|| "controller guard deadline exceeds the platform clock range".to_owned())?;
    let release = run_dir.join("controller-release");
    while !release.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_secs(config.interval_seconds).min(Duration::from_secs(5)));
    }
    let timed_out = !release.is_file();
    let cleanup = systemctl_action(
        "stop",
        &config.controller_unit,
        repo,
        config.command_timeout_seconds,
    );
    let inactive = systemd_properties(
        &config.controller_unit,
        repo,
        config.command_timeout_seconds,
    )
    .and_then(|values| {
        (values.get("ActiveState").map(String::as_str) == Some("inactive"))
            .then_some(())
            .ok_or_else(|| "controller did not return to inactive state".to_owned())
    });
    match (timed_out, cleanup, inactive) {
        (false, Ok(()), Ok(())) => write_root_guard_status(
            run_dir,
            config,
            "stopped",
            "controller stopped and the pre-run inactive state was restored",
        ),
        (deadline_elapsed, stop, state) => {
            let message = format!(
                "controller cleanup failed: deadline_elapsed={deadline_elapsed} stop={stop:?} inactive={state:?}"
            );
            write_root_guard_status(run_dir, config, "failed", &message)?;
            Err(message)
        }
    }
}

fn run_internal(args: &[String], repo: &Path) -> Result<(), String> {
    if args.len() != 2 || args[0] != "--run-dir" {
        return Err("qualification run is an internal command requiring --run-dir PATH".to_owned());
    }
    let run_dir = PathBuf::from(&args[1]);
    let config: Config = serde_json::from_str(&process::read_file(&run_dir.join("config.json"))?)
        .map_err(|e| format!("parse qualification config: {e}"))?;
    wait_for_launcher_status(
        &run_dir.join("status.json"),
        Duration::from_secs(config.command_timeout_seconds),
        Duration::from_secs(config.interval_seconds),
    )?;
    let started = now_millis()?;
    let mut status = Status {
        version: SCHEMA_VERSION,
        run_id: config.run_id.clone(),
        state: "running".to_owned(),
        pid: Some(std::process::id()),
        started_unix_millis: Some(started),
        finished_unix_millis: None,
        final_result: None,
        message: "preflight".to_owned(),
    };
    write_json(&run_dir.join("status.json"), &status)?;
    event(
        &run_dir,
        "info",
        "run_started",
        json!({"configuration": config}),
    )?;
    let result = supervise(&config, &run_dir, repo);
    let finished = now_millis()?;
    status.state = "finished".to_owned();
    status.finished_unix_millis = Some(finished);
    status.final_result = Some(if result.is_ok() { "pass" } else { "fail" }.to_owned());
    status.message = result.as_ref().map_or_else(
        |e| e.clone(),
        |_| "qualification criteria passed".to_owned(),
    );
    write_json(&run_dir.join("status.json"), &status)?;
    let observations = process::read_file(&run_dir.join("result.json"))
        .ok()
        .and_then(|value| serde_json::from_str::<Value>(&value).ok());
    let artifacts = available_artifacts(&run_dir);
    let summary = json!({"version": SCHEMA_VERSION, "run_id": config.run_id, "test": "automatic_controller_workload_resize", "started_unix_millis": started, "finished_unix_millis": finished, "duration_millis": finished.saturating_sub(started), "status": status.final_result, "message": status.message, "observations": observations, "artifacts": artifacts});
    write_json(&run_dir.join("summary.json"), &summary)?;
    event(
        &run_dir,
        if result.is_ok() { "info" } else { "failure" },
        "run_finished",
        summary,
    )?;
    result
}

fn supervise(config: &Config, run_dir: &Path, repo: &Path) -> Result<(), String> {
    let result = supervise_inner(config, run_dir, repo);
    let release = write_release(run_dir);
    let cleanup = wait_for_guard_stopped(
        run_dir,
        config,
        Duration::from_secs(config.command_timeout_seconds)
            .checked_mul(3)
            .ok_or_else(|| "controller cleanup timeout overflowed".to_owned())?,
        Duration::from_secs(config.interval_seconds),
    );
    archive_controller_log(config, run_dir, repo);
    let final_state = capture_live_state(config, repo, "after_controller")
        .and_then(|state| write_json(&run_dir.join("final-state.json"), &state));
    match (result, release, cleanup, final_state) {
        (Ok(()), Ok(()), Ok(()), Ok(())) => Ok(()),
        (Err(error), _, Ok(()), Ok(())) => Err(error),
        (result, release, cleanup, final_state) => Err(format!(
            "qualification result={result:?}; controller release={release:?}; cleanup={cleanup:?}; final_state={final_state:?}"
        )),
    }
}

fn supervise_inner(config: &Config, run_dir: &Path, repo: &Path) -> Result<(), String> {
    for command in ["ssh", "virsh"] {
        if !process::command_exists(command) {
            return Err(format!("missing prerequisite: {command}"));
        }
    }
    preflight_health(config, run_dir, repo, "before_workload")?;
    let mut observations = Observations::default();
    sample_host(config, run_dir, repo, "baseline", &mut observations)?;
    let remote = workload_command(config);
    event(
        run_dir,
        "info",
        "workload_start",
        json!({"ssh_target": config.ssh_target, "command": remote}),
    )?;
    let connect_timeout = format!("ConnectTimeout={}", config.command_timeout_seconds);
    let mut child = ProcessCommand::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            &connect_timeout,
            "--",
            &config.ssh_target,
            &remote,
        ])
        .current_dir(repo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("start remote workload: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "capture workload stdout".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "capture workload stderr".to_owned())?;
    let (sender, receiver) = mpsc::channel();
    spawn_reader(stdout, false, sender.clone());
    spawn_reader(stderr, true, sender);
    let started = Instant::now();
    let max_duration_seconds = config
        .peak_hold_seconds
        .checked_add(config.settled_hold_seconds)
        .and_then(|value| value.checked_add(config.renewed_hold_seconds))
        .and_then(|value| value.checked_add(config.command_timeout_seconds))
        .ok_or_else(|| "configured workload deadline overflows".to_owned())?;
    let max_duration = Duration::from_secs(max_duration_seconds);
    let interval = Duration::from_secs(config.interval_seconds);
    let mut next_sample = Instant::now() + interval;
    let mut phase = "baseline".to_owned();
    let exit_status;
    loop {
        while let Ok((is_error, line)) = receiver.try_recv() {
            if is_error {
                observations.warnings += 1;
                event(
                    run_dir,
                    "warning",
                    "workload_stderr",
                    json!({"message": line}),
                )?;
            } else {
                append_line(&run_dir.join("workload.jsonl"), &line)?;
                let parsed: Value = serde_json::from_str(&line)
                    .map_err(|e| format!("workload emitted invalid JSON: {e}: {line}"))?;
                phase = parsed
                    .get("phase")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "workload record has no phase".to_owned())?
                    .to_owned();
                observations.workload_phases.push(phase.clone());
                event(run_dir, "info", "workload_phase", parsed)?;
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("poll workload: {e}"))?
        {
            exit_status = status;
            break;
        }
        if started.elapsed() > max_duration {
            let _ = child.kill();
            return Err("workload exceeded its bounded deadline".to_owned());
        }
        if Instant::now() >= next_sample {
            if let Err(error) = sample_host(config, run_dir, repo, &phase, &mut observations) {
                observations.warnings += 1;
                event(
                    run_dir,
                    "warning",
                    "host_sample_failed",
                    json!({"error": error}),
                )?;
            }
            next_sample = Instant::now() + interval;
        }
        let wait = next_sample.saturating_duration_since(Instant::now());
        if let Ok((is_error, line)) = receiver.recv_timeout(wait) {
            if is_error {
                observations.warnings += 1;
                event(
                    run_dir,
                    "warning",
                    "workload_stderr",
                    json!({"message": line}),
                )?;
            } else {
                append_line(&run_dir.join("workload.jsonl"), &line)?;
                let parsed: Value = serde_json::from_str(&line)
                    .map_err(|e| format!("workload emitted invalid JSON: {e}: {line}"))?;
                phase = parsed
                    .get("phase")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "workload record has no phase".to_owned())?
                    .to_owned();
                observations.workload_phases.push(phase.clone());
                event(run_dir, "info", "workload_phase", parsed)?;
            }
        }
    }
    while let Ok((is_error, line)) = receiver.try_recv() {
        if is_error {
            observations.warnings += 1;
            event(
                run_dir,
                "warning",
                "workload_stderr",
                json!({"message": line}),
            )?;
        } else {
            append_line(&run_dir.join("workload.jsonl"), &line)?;
            if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                if let Some(value) = parsed.get("phase").and_then(Value::as_str) {
                    observations.workload_phases.push(value.to_owned());
                }
                event(run_dir, "info", "workload_phase", parsed)?;
            }
        }
    }
    if !exit_status.success() {
        return Err(format!("remote workload exited with {exit_status}"));
    }
    let post_deadline = Instant::now() + Duration::from_secs(config.post_hold_seconds);
    while Instant::now() < post_deadline {
        sample_host(config, run_dir, repo, "post_workload", &mut observations)?;
        thread::sleep(interval.min(post_deadline.saturating_duration_since(Instant::now())));
    }
    preflight_health(config, run_dir, repo, "after_workload")?;
    let initial = observations.initial_current.unwrap_or(0);
    let growth = observations.maximum_peak_current.saturating_sub(initial);
    let reclaimed = observations.minimum_settled_current.map_or(0, |minimum| {
        observations.maximum_peak_current.saturating_sub(minimum)
    });
    write_json(
        &run_dir.join("result.json"),
        &json!({
            "version": SCHEMA_VERSION,
            "sample_count": observations.sample_count,
            "warning_count": observations.warnings,
            "initial_current_bytes": observations.initial_current,
            "maximum_current_bytes": observations.maximum_current,
            "maximum_peak_current_bytes": observations.maximum_peak_current,
            "minimum_settled_current_bytes": observations.minimum_settled_current,
            "observed_growth_bytes": growth,
            "required_growth_bytes": config.expect_growth_bytes,
            "observed_reclaim_bytes": reclaimed,
            "required_reclaim_bytes": config.expect_reclaim_bytes,
            "workload_phases": observations.workload_phases,
        }),
    )?;
    classify(config, &observations)
}

fn sample_host(
    config: &Config,
    run_dir: &Path,
    repo: &Path,
    phase: &str,
    observations: &mut Observations,
) -> Result<(), String> {
    let xml = virsh(config, repo, &["dumpxml", &config.vm_name])?;
    let memory = parse_virtio_mem_xml_for_alias(&xml, &config.device_alias)
        .map_err(|e| format!("parse virtio-mem state: {e}"))?
        .memory;
    let domstate = virsh(config, repo, &["domstate", &config.vm_name])?;
    if !domstate.to_ascii_lowercase().contains("running") {
        return Err(format!("VM is not running: {}", domstate.trim()));
    }
    let dommemstat = virsh(config, repo, &["dommemstat", &config.vm_name])?;
    let host = process::read_file(Path::new("/proc/meminfo"))?;
    let host_available_bytes = meminfo_value(&host, "MemAvailable:");
    let guest_stats = whitespace_pairs(&dommemstat);
    let reader = VirshQgaFileReader::new(
        Virsh::with_connection(
            "virsh",
            Duration::from_secs(config.command_timeout_seconds),
            &config.connect_uri,
        ),
        &config.vm_name,
    );
    let bytes = reader.read_file(
        &config.telemetry_path,
        virtio_mem_host::raw_telemetry::MAX_RAW_TELEMETRY_FILE_BYTES as usize,
    )?;
    let contents = String::from_utf8(bytes)
        .map_err(|error| format!("QGA raw telemetry is not valid UTF-8: {error}"))?;
    let line = last_complete_record(&contents)
        .ok_or_else(|| "configured Windows telemetry has no complete record".to_owned())?;
    let telemetry: RawTelemetryEnvelope = serde_json::from_str(line)
        .map_err(|error| format!("configured Windows telemetry is invalid JSON: {error}"))?;
    let now = u64::try_from(now_millis()?)
        .map_err(|_| "current Unix timestamp does not fit in u64 milliseconds".to_owned())?;
    telemetry
        .validate_for(
            &config.vm_name,
            &config.guest_service,
            now,
            seconds_millis(config.telemetry_max_age_seconds)?,
            seconds_millis(config.telemetry_future_tolerance_seconds)?,
        )
        .map_err(|error| format!("configured Windows telemetry failed validation: {error}"))?;
    let sample = json!({"version": SCHEMA_VERSION, "run_id": config.run_id, "unix_millis": now_millis()?, "workload_phase": phase, "vm_state": domstate.trim(), "requested_bytes": memory.requested_bytes, "current_bytes": memory.current_bytes, "device_size_bytes": memory.size_bytes, "block_size_bytes": memory.block_size_bytes, "host_mem_available_bytes": host_available_bytes, "guest_dommemstat_kib": guest_stats, "windows_raw_telemetry": telemetry});
    append_json(&run_dir.join("host-metrics.jsonl"), &sample)?;
    if let Some(previous) = observations.last_requested {
        if previous != memory.requested_bytes {
            event(
                run_dir,
                "info",
                "resize_request_observed",
                json!({
                    "previous_requested_bytes": previous,
                    "requested_bytes": memory.requested_bytes,
                    "current_bytes": memory.current_bytes,
                    "workload_phase": phase,
                }),
            )?;
        }
    }
    observations.last_requested = Some(memory.requested_bytes);
    observations
        .initial_current
        .get_or_insert(memory.current_bytes);
    observations.maximum_current = observations.maximum_current.max(memory.current_bytes);
    if phase == "peak" {
        observations.maximum_peak_current =
            observations.maximum_peak_current.max(memory.current_bytes);
    }
    if matches!(phase, "settled" | "post_workload" | "complete") {
        observations.minimum_settled_current = Some(
            observations
                .minimum_settled_current
                .map_or(memory.current_bytes, |value| {
                    value.min(memory.current_bytes)
                }),
        );
    }
    observations.sample_count += 1;
    Ok(())
}

fn preflight_health(
    config: &Config,
    run_dir: &Path,
    repo: &Path,
    stage: &str,
) -> Result<(), String> {
    let timeout = Duration::from_secs(config.command_timeout_seconds);
    let controller = process::bounded_text(
        "systemctl",
        &[
            OsString::from("is-active"),
            OsString::from(&config.controller_unit),
        ],
        repo,
        timeout,
    )?;
    let guest = guest_health(config, repo, stage)?;
    event(
        run_dir,
        "info",
        "preflight_health_passed",
        json!({"controller": controller.trim(), "guest": guest}),
    )
}

fn preflight_guest_health(
    config: &Config,
    run_dir: &Path,
    repo: &Path,
    stage: &str,
) -> Result<(), String> {
    let guest = guest_health(config, repo, stage)?;
    event(run_dir, "info", "guest_health_passed", guest)
}

fn guest_health(config: &Config, repo: &Path, stage: &str) -> Result<Value, String> {
    let ping = virsh(
        config,
        repo,
        &[
            "qemu-agent-command",
            &config.vm_name,
            r#"{"execute":"guest-ping"}"#,
        ],
    )?;
    let service = remote_text(config, repo, &format!("sc query {}", config.guest_service))?;
    if !service.contains("RUNNING") {
        return Err(format!(
            "guest service {} is not RUNNING",
            config.guest_service
        ));
    }
    let installers = remote_text(config, repo, r#"tasklist /FI "IMAGENAME eq msiexec.exe""#)?;
    if installers.to_ascii_lowercase().contains("msiexec.exe") {
        return Err("Windows Installer is running; refusing to overlap qualification".to_owned());
    }
    for key in [
        r#"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending"#,
        r#"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired"#,
    ] {
        let result = remote_text(
            config,
            repo,
            &format!(r#"reg query "{key}" 2>nul || echo NOT_PENDING"#),
        )?;
        if !result.contains("NOT_PENDING") {
            return Err(format!("Windows pending-reboot marker exists: {key}"));
        }
    }
    Ok(json!({
        "stage": stage,
        "qga_reply": ping.trim(),
        "guest_service": config.guest_service,
        "authenticated_guest_command": true,
        "installer_running": false,
        "pending_reboot": false,
    }))
}

fn remote_text(config: &Config, repo: &Path, command: &str) -> Result<String, String> {
    let connect_timeout = format!("ConnectTimeout={}", config.command_timeout_seconds);
    process::bounded_text(
        "ssh",
        &[
            OsString::from("-o"),
            OsString::from("BatchMode=yes"),
            OsString::from("-o"),
            OsString::from(connect_timeout),
            OsString::from("--"),
            OsString::from(&config.ssh_target),
            OsString::from(command),
        ],
        repo,
        Duration::from_secs(config.command_timeout_seconds),
    )
}

fn classify(config: &Config, observations: &Observations) -> Result<(), String> {
    let required = ["baseline", "peak", "settled", "renewed", "complete"];
    for phase in required {
        if !observations
            .workload_phases
            .iter()
            .any(|value| value == phase)
        {
            return Err(format!("missing workload phase: {phase}"));
        }
    }
    let initial = observations
        .initial_current
        .ok_or_else(|| "no initial memory sample".to_owned())?;
    let growth = observations.maximum_peak_current.saturating_sub(initial);
    let reclaimed = observations.minimum_settled_current.map_or(0, |minimum| {
        observations.maximum_peak_current.saturating_sub(minimum)
    });
    if growth < config.expect_growth_bytes {
        return Err(format!(
            "observed growth {growth} bytes is below required {}",
            config.expect_growth_bytes
        ));
    }
    if reclaimed < config.expect_reclaim_bytes {
        return Err(format!(
            "observed reclaim {reclaimed} bytes is below required {}",
            config.expect_reclaim_bytes
        ));
    }
    Ok(())
}

fn workload_command(config: &Config) -> String {
    let mode = match config.mode {
        WorkloadMode::Committed => "committed",
        WorkloadMode::Resident => "resident",
    };
    format!("\"{}\" --workload-id {} --mode {mode} --peak-bytes {} --retained-bytes {} --max-allocation-bytes {} --peak-hold-seconds {} --settled-hold-seconds {} --renewed-hold-seconds {} --refresh-interval-seconds {}", config.remote_workload, config.run_id, config.peak_bytes, config.retained_bytes, config.max_allocation_bytes, config.peak_hold_seconds, config.settled_hold_seconds, config.renewed_hold_seconds, config.resident_refresh_seconds)
}

fn archive_controller_log(config: &Config, run_dir: &Path, repo: &Path) {
    let status_path = run_dir.join("status.json");
    let Some(since) = process::read_file(&status_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Status>(&s).ok())
        .and_then(|s| s.started_unix_millis)
        .map(|v| format!("@{}", v / 1000))
    else {
        let _ = event(
            run_dir,
            "warning",
            "controller_log_failed",
            json!({"error": "run start time is unavailable; refusing to guess a journal window"}),
        );
        return;
    };
    let args = [
        OsString::from("--no-pager"),
        OsString::from("--output=short-iso"),
        OsString::from("--unit"),
        OsString::from(&config.controller_unit),
        OsString::from("--since"),
        OsString::from(since),
    ];
    match process::bounded_text(
        "journalctl",
        &args,
        repo,
        Duration::from_secs(config.command_timeout_seconds),
    ) {
        Ok(value) => {
            let _ = std::fs::write(run_dir.join("controller.log"), value);
        }
        Err(error) => {
            let _ = event(
                run_dir,
                "warning",
                "controller_log_failed",
                json!({"error": error}),
            );
        }
    }
}

fn show_status(args: &[String], repo: &Path) -> Result<(), String> {
    let run_dir = locate_run(args, repo)?;
    let status = process::read_file(&run_dir.join("status.json"))?;
    let parsed: Status =
        serde_json::from_str(&status).map_err(|e| format!("parse run status: {e}"))?;
    println!("{status}\noutput_dir={}", run_dir.display());
    if parsed.state != "finished"
        && parsed
            .pid
            .is_some_and(|pid| !Path::new("/proc").join(pid.to_string()).exists())
    {
        println!("warning=supervisor process is no longer present; inspect supervisor.log for an interrupted run");
    }
    Ok(())
}

fn review(args: &[String], repo: &Path) -> Result<(), String> {
    let run_dir = locate_run(args, repo)?;
    let summary = process::read_file(&run_dir.join("summary.json")).unwrap_or_else(|_| {
        process::read_file(&run_dir.join("status.json"))
            .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    });
    println!("{summary}\noutput_dir={}", run_dir.display());
    for name in [
        "result.json",
        "initial-state.json",
        "final-state.json",
        "events.jsonl",
        "host-metrics.jsonl",
        "workload.jsonl",
        "controller.log",
        "controller-guard.json",
        "supervisor.log",
    ] {
        let path = run_dir.join(name);
        if path.is_file() {
            println!("artifact={}", path.display());
        }
    }
    Ok(())
}

fn locate_run(args: &[String], repo: &Path) -> Result<PathBuf, String> {
    if args.is_empty() {
        return Err("RUN_ID is required".to_owned());
    }
    let run_id = identifier(&args[0], "RUN_ID")?;
    let mut root = repo.join(".artifacts/qualification");
    if args.len() == 3 && args[1] == "--output-root" {
        root = absolute_or_repo(repo, &args[2]);
    } else if args.len() != 1 {
        return Err("expected RUN_ID [--output-root PATH]".to_owned());
    }
    Ok(root.join(run_id))
}

fn virsh(config: &Config, repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut values = vec![OsString::from("-c"), OsString::from(&config.connect_uri)];
    values.extend(args.iter().map(OsString::from));
    process::bounded_text(
        "virsh",
        &values,
        repo,
        Duration::from_secs(config.command_timeout_seconds),
    )
}

fn systemctl_action(
    action: &str,
    unit: &str,
    repo: &Path,
    timeout_seconds: u64,
) -> Result<(), String> {
    process::bounded_text(
        "systemctl",
        &[OsString::from(action), OsString::from(unit)],
        repo,
        Duration::from_secs(timeout_seconds),
    )
    .map(|_| ())
}

fn capture_live_state(
    config: &Config,
    repo: &Path,
    stage: &str,
) -> Result<LiveStateEvidence, String> {
    let xml = virsh(config, repo, &["dumpxml", &config.vm_name])?;
    let memory = parse_virtio_mem_xml_for_alias(&xml, &config.device_alias)
        .map_err(|error| format!("parse virtio-mem state: {error}"))?
        .memory;
    let vm_state = virsh(config, repo, &["domstate", &config.vm_name])?;
    if !vm_state.to_ascii_lowercase().contains("running") {
        return Err(format!("VM is not running: {}", vm_state.trim()));
    }
    let properties = systemd_properties(
        &config.controller_unit,
        repo,
        config.command_timeout_seconds,
    )?;
    let host = process::read_file(Path::new("/proc/meminfo"))?;
    Ok(LiveStateEvidence {
        version: SCHEMA_VERSION,
        run_id: config.run_id.clone(),
        stage: stage.to_owned(),
        unix_millis: now_millis()?,
        vm_state: vm_state.trim().to_owned(),
        controller_active_state: properties
            .get("ActiveState")
            .cloned()
            .ok_or_else(|| "systemctl did not report ActiveState".to_owned())?,
        controller_unit_file_state: properties
            .get("UnitFileState")
            .cloned()
            .ok_or_else(|| "systemctl did not report UnitFileState".to_owned())?,
        requested_bytes: memory.requested_bytes,
        current_bytes: memory.current_bytes,
        device_size_bytes: memory.size_bytes,
        block_size_bytes: memory.block_size_bytes,
        host_mem_available_bytes: meminfo_value(&host, "MemAvailable:"),
    })
}

fn systemd_properties(
    unit: &str,
    repo: &Path,
    timeout_seconds: u64,
) -> Result<BTreeMap<String, String>, String> {
    let output = process::bounded_text(
        "systemctl",
        &[
            OsString::from("show"),
            OsString::from(unit),
            OsString::from("--property=ActiveState,UnitFileState,MainPID"),
            OsString::from("--no-pager"),
        ],
        repo,
        Duration::from_secs(timeout_seconds),
    )?;
    let mut properties = BTreeMap::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("systemctl returned malformed property: {line}"))?;
        if properties
            .insert(key.to_owned(), value.to_owned())
            .is_some()
        {
            return Err(format!("systemctl returned repeated property: {key}"));
        }
    }
    Ok(properties)
}

fn require_controller_active_and_shrink(config: &Config, repo: &Path) -> Result<(), String> {
    let properties = systemd_properties(
        &config.controller_unit,
        repo,
        config.command_timeout_seconds,
    )?;
    if properties.get("ActiveState").map(String::as_str) != Some("active") {
        return Err(format!(
            "controller {} did not become active",
            config.controller_unit
        ));
    }
    let pid = properties
        .get("MainPID")
        .ok_or_else(|| "systemctl did not report MainPID".to_owned())?
        .parse::<u32>()
        .map_err(|_| "systemctl returned an invalid MainPID".to_owned())?;
    if pid == 0 {
        return Err("controller has no running MainPID".to_owned());
    }
    let environment = std::fs::read(format!("/proc/{pid}/environ"))
        .map_err(|error| format!("read controller environment: {error}"))?;
    let shrink = environment.split(|byte| *byte == 0).find_map(|entry| {
        entry
            .strip_prefix(b"VIRTIO_MEM_AUTOMATIC_WINDOWS_SHRINK=")
            .map(|value| String::from_utf8_lossy(value).into_owned())
    });
    if shrink.as_deref().is_some_and(|value| value != "true") {
        return Err(format!(
            "automatic Windows shrink is not enabled for qualification: {shrink:?}"
        ));
    }
    Ok(())
}

fn write_root_guard_status(
    _run_dir: &Path,
    config: &Config,
    state: &str,
    message: &str,
) -> Result<(), String> {
    let root = Path::new(CONTROLLER_GUARD_ROOT);
    std::fs::create_dir_all(root)
        .map_err(|error| format!("create controller guard state directory: {error}"))?;
    write_json(
        &root.join(format!("{}.json", config.run_id)),
        &ControllerGuardStatus {
            version: SCHEMA_VERSION,
            run_id: config.run_id.clone(),
            state: state.to_owned(),
            message: message.to_owned(),
        },
    )
}

fn write_release(run_dir: &Path) -> Result<(), String> {
    let mut file = File::create(run_dir.join("controller-release"))
        .map_err(|error| format!("create controller release marker: {error}"))?;
    file.write_all(b"release\n")
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("publish controller release marker: {error}"))
}

fn finish_start_failure(run_dir: &Path, mut status: Status, error: &str) -> Result<(), String> {
    status.state = "finished".to_owned();
    status.finished_unix_millis = Some(now_millis()?);
    status.final_result = Some("fail".to_owned());
    status.message = error.to_owned();
    write_json(&run_dir.join("status.json"), &status)
}

fn wait_for_guard_stopped(
    run_dir: &Path,
    config: &Config,
    timeout: Duration,
    interval: Duration,
) -> Result<(), String> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| "controller cleanup wait exceeds the platform clock range".to_owned())?;
    loop {
        if let Ok(value) = process::read_file(
            &Path::new(CONTROLLER_GUARD_ROOT).join(format!("{}.json", config.run_id)),
        ) {
            if let Ok(status) = serde_json::from_str::<ControllerGuardStatus>(&value) {
                if let Some(result) = guard_status_result(&status) {
                    write_json(&run_dir.join("controller-guard.json"), &status)?;
                    return result;
                }
            }
        }
        if Instant::now() >= deadline {
            return Err("controller guard did not confirm cleanup before the bound".to_owned());
        }
        thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
    }
}

fn guard_status_result(status: &ControllerGuardStatus) -> Option<Result<(), String>> {
    match status.state.as_str() {
        "stopped" => Some(Ok(())),
        "failed" => Some(Err(status.message.clone())),
        _ => None,
    }
}

unsafe extern "C" {
    fn geteuid() -> u32;
}

unsafe fn libc_geteuid() -> u32 {
    unsafe { geteuid() }
}

fn spawn_reader(
    input: impl std::io::Read + Send + 'static,
    is_error: bool,
    sender: mpsc::Sender<(bool, String)>,
) {
    thread::spawn(move || {
        for line in BufReader::new(input).lines().map_while(Result::ok) {
            let _ = sender.send((is_error, line));
        }
    });
}

fn event(run_dir: &Path, level: &str, kind: &str, detail: Value) -> Result<(), String> {
    append_json(
        &run_dir.join("events.jsonl"),
        &json!({"version": SCHEMA_VERSION, "unix_millis": now_millis()?, "level": level, "event": kind, "detail": detail}),
    )
}

fn append_json(path: &Path, value: &Value) -> Result<(), String> {
    append_line(
        path,
        &serde_json::to_string(value).map_err(|e| format!("serialize evidence: {e}"))?,
    )
}
fn append_line(path: &Path, line: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    writeln!(file, "{line}")
        .and_then(|_| file.flush())
        .map_err(|e| format!("write {}: {e}", path.display()))
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    let mut file =
        File::create(&temporary).map_err(|e| format!("create {}: {e}", temporary.display()))?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|e| format!("encode {}: {e}", path.display()))?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("flush {}: {e}", path.display()))?;
    std::fs::rename(&temporary, path).map_err(|e| format!("publish {}: {e}", path.display()))
}
fn now_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis())
        .map_err(|e| format!("system clock before Unix epoch: {e}"))
}
fn new_run_id() -> Result<String, String> {
    Ok(format!(
        "qualification-{}-{}",
        now_millis()?,
        std::process::id()
    ))
}
fn value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .filter(|v| !v.is_empty())
        .cloned()
        .ok_or_else(|| format!("{option} requires a value"))
}
fn number(args: &[String], index: &mut usize, option: &str) -> Result<u64, String> {
    value(args, index, option)?
        .parse()
        .map_err(|_| format!("{option} requires an unsigned integer"))
}
fn required<T>(value: Option<T>, option: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{option} is required"))
}
fn scope(value: &str, name: &str) -> Result<String, String> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        Err(format!("{name} is invalid"))
    } else {
        Ok(value.to_owned())
    }
}
fn identifier(value: &str, name: &str) -> Result<String, String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'@'))
    {
        Err(format!("{name} contains unsafe characters"))
    } else {
        Ok(value.to_owned())
    }
}
fn absolute_or_repo(repo: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo.join(path)
    }
}
fn meminfo_value(input: &str, name: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some(name))
            .then(|| fields.next()?.parse::<u64>().ok()?.checked_mul(1024))
            .flatten()
    })
}
fn whitespace_pairs(input: &str) -> serde_json::Map<String, Value> {
    input
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((
                parts.next()?.to_owned(),
                Value::from(parts.next()?.parse::<u64>().ok()?),
            ))
        })
        .collect()
}
fn last_complete_record(value: &str) -> Option<&str> {
    if !value.ends_with('\n') {
        return None;
    }
    value.lines().rev().find(|line| !line.trim().is_empty())
}

fn seconds_millis(seconds: u64) -> Result<u64, String> {
    seconds
        .checked_mul(1000)
        .ok_or_else(|| "telemetry time bound overflows milliseconds".to_owned())
}

fn available_artifacts(run_dir: &Path) -> Vec<&'static str> {
    let mut names = vec!["summary.json"];
    names.extend(
        [
            "config.json",
            "status.json",
            "result.json",
            "initial-state.json",
            "final-state.json",
            "events.jsonl",
            "host-metrics.jsonl",
            "workload.jsonl",
            "controller.log",
            "controller-guard.json",
            "supervisor.log",
        ]
        .into_iter()
        .filter(|name| run_dir.join(name).is_file()),
    );
    names
}

fn wait_for_launcher_status(
    path: &Path,
    timeout: Duration,
    interval: Duration,
) -> Result<(), String> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| "launcher timeout exceeds the platform clock range".to_owned())?;
    loop {
        if process::read_file(path)
            .ok()
            .and_then(|value| serde_json::from_str::<Status>(&value).ok())
            .and_then(|status| status.pid)
            .is_some()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("launcher did not publish the detached supervisor PID".to_owned());
        }
        thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    fn valid_start_arguments() -> Vec<String> {
        strings(&[
            "vm",
            "memory0",
            "--ssh-target",
            "guest",
            "--mode",
            "resident",
            "--peak-bytes",
            "8192",
            "--retained-bytes",
            "4096",
            "--max-allocation-bytes",
            "16384",
            "--peak-hold-seconds",
            "2",
            "--settled-hold-seconds",
            "3",
            "--renewed-hold-seconds",
            "2",
            "--resident-refresh-seconds",
            "1",
            "--post-hold-seconds",
            "1",
            "--interval-seconds",
            "1",
            "--command-timeout-seconds",
            "4",
            "--controller-timeout-seconds",
            "60",
            "--expect-growth-bytes",
            "2048",
            "--expect-reclaim-bytes",
            "1024",
            "--remote-workload",
            r"C:\qualification\virtio-mem-workload.exe",
            "--controller-unit",
            "virtio-mem-host@vm.service",
            "--guest-service",
            "ConfiguredService",
            "--telemetry-path",
            r"C:\ProgramData\VirtioMemService\telemetry.json",
            "--telemetry-max-age-seconds",
            "30",
            "--telemetry-future-tolerance-seconds",
            "5",
        ])
    }

    #[test]
    fn explicit_configuration_is_dry_run_until_apply() {
        let repo = Path::new("/repo");
        let parsed = parse_start(&valid_start_arguments(), repo).expect("configuration");
        assert!(!parsed.apply);
        assert_eq!(parsed.config.mode, WorkloadMode::Resident);
        assert_eq!(parsed.config.peak_bytes, 8192);
        assert_eq!(parsed.config.expect_reclaim_bytes, 1024);
    }

    #[test]
    fn rejects_implicit_or_zero_operational_settings() {
        assert!(parse_start(&strings(&["vm", "memory0"]), Path::new("/repo")).is_err());
        let mut invalid = valid_start_arguments();
        let position = invalid
            .iter()
            .position(|value| value == "--command-timeout-seconds")
            .expect("timeout option");
        invalid[position + 1] = "0".to_owned();
        assert!(parse_start(&invalid, Path::new("/repo")).is_err());
    }

    #[test]
    fn apply_requires_the_typed_elevation_boundary() {
        let mut arguments = valid_start_arguments();
        arguments.push("--apply".to_owned());
        let error = parse_start(&arguments, Path::new("/repo")).expect_err("missing elevation");
        assert!(error.contains("requires --elevate"));
        arguments.push("--elevate".to_owned());
        assert!(parse_start(&arguments, Path::new("/repo")).is_ok());
    }

    #[test]
    fn rejects_short_controller_ownership_bound_and_non_windows_telemetry_path() {
        let mut arguments = valid_start_arguments();
        let timeout = arguments
            .iter()
            .position(|value| value == "--controller-timeout-seconds")
            .expect("controller timeout");
        arguments[timeout + 1] = "47".to_owned();
        assert!(parse_start(&arguments, Path::new("/repo")).is_err());

        let mut arguments = valid_start_arguments();
        let telemetry = arguments
            .iter()
            .position(|value| value == "--telemetry-path")
            .expect("telemetry path");
        arguments[telemetry + 1] = "/run/telemetry.jsonl".to_owned();
        assert!(parse_start(&arguments, Path::new("/repo")).is_err());
    }

    #[test]
    fn guard_completion_is_a_required_cleanup_signal() {
        let status = |state: &str, message: &str| ControllerGuardStatus {
            version: SCHEMA_VERSION,
            run_id: "run".to_owned(),
            state: state.to_owned(),
            message: message.to_owned(),
        };
        assert_eq!(guard_status_result(&status("running", "active")), None);
        assert_eq!(
            guard_status_result(&status("stopped", "restored")),
            Some(Ok(()))
        );
        assert_eq!(
            guard_status_result(&status("failed", "stop failed")),
            Some(Err("stop failed".to_owned()))
        );
    }

    #[test]
    fn result_requires_all_phases_and_resize_thresholds() {
        let mut options =
            parse_start(&valid_start_arguments(), Path::new("/repo")).expect("options");
        options.config.expect_growth_bytes = 100;
        options.config.expect_reclaim_bytes = 50;
        let observations = Observations {
            initial_current: Some(1_000),
            maximum_current: 1_200,
            maximum_peak_current: 1_200,
            minimum_settled_current: Some(1_100),
            workload_phases: ["baseline", "peak", "settled", "renewed", "complete"]
                .iter()
                .map(|v| (*v).to_owned())
                .collect(),
            sample_count: 5,
            warnings: 0,
            last_requested: Some(1_100),
        };
        assert!(classify(&options.config, &observations).is_ok());
        options.config.expect_growth_bytes = 201;
        assert!(classify(&options.config, &observations).is_err());
    }

    #[test]
    fn parses_host_memory_and_dommemstat_without_guessing_units() {
        assert_eq!(
            meminfo_value("MemAvailable: 1024 kB\n", "MemAvailable:"),
            Some(1 << 20)
        );
        let values = whitespace_pairs("unused 10\navailable 20\nlast-update 30\n");
        assert_eq!(values["available"], 20);
    }

    #[test]
    fn workload_command_preserves_mode_bounds_and_run_identity() {
        let mut arguments = valid_start_arguments();
        let position = arguments
            .iter()
            .position(|value| value == "--mode")
            .expect("mode option");
        arguments[position + 1] = "committed".to_owned();
        let options = parse_start(&arguments, Path::new("/repo")).expect("options");
        let command = workload_command(&options.config);
        assert!(command.contains("--mode committed"));
        assert!(command.contains(&format!("--workload-id {}", options.config.run_id)));
        assert!(command.contains("--max-allocation-bytes 16384"));
        assert!(command.contains("--refresh-interval-seconds 1"));
    }

    #[test]
    fn artifact_index_only_claims_files_that_exist() {
        let directory = std::env::temp_dir().join(format!(
            "virtio-mem-qualification-artifacts-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("create test directory");
        std::fs::write(directory.join("status.json"), b"{}\n").expect("write status");
        let artifacts = available_artifacts(&directory);
        assert!(artifacts.contains(&"summary.json"));
        assert!(artifacts.contains(&"status.json"));
        assert!(!artifacts.contains(&"result.json"));
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
