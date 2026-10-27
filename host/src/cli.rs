use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use virtio_mem_core::{
    parse_virtio_mem_xml_for_alias, AbandonAction, AbandonToCurrent, GuestInputStatus,
    ShrinkObservation,
};

use crate::attestation::{
    read_review, AttestedCompatibilitySource, CompatibilityAttestation,
    VirshCompatibilityEvidenceSource,
};
use crate::checkpoint::{clear_actuation_latch, read_controller_status};
use crate::compatibility_source::CompatibilitySource;
use crate::config::{HostConfig, RawTelemetryTransport};
use crate::guest_channel::{GuestChannel, GuestLink, SystemUnixClock, Transport, UnixClock};
use crate::host_memory::{validate_grow_headroom, HostMemorySource, ProcMeminfoSource};
use crate::qga::VirshGuestFiles;
use crate::resize_sink::VirshResizeSink;
use crate::runtime::MemoryStateSource;
use crate::virsh::{Virsh, VirshCommand};
use crate::xml_source::VirshXmlSource;

const DEFAULT_CONNECTION: &str = "qemu:///system";

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    /// Run the controller daemon.
    Run { config: PathBuf },
    Status {
        config: PathBuf,
        vm: Option<String>,
        connection: String,
    },
    Attest {
        vm: String,
        alias: String,
        review_path: String,
        connection: String,
        command_timeout_seconds: u64,
    },
    Snapshot {
        vm: String,
        alias: String,
        connection: String,
        command_timeout_seconds: u64,
    },
    Validate {
        vm: String,
        alias: String,
        connection: String,
        command_timeout_seconds: u64,
    },
    Resize {
        vm: String,
        alias: String,
        target_bytes: u64,
        connection: String,
        apply: bool,
        attestation_path: String,
        host_min_headroom_bytes: u64,
        command_timeout_seconds: u64,
    },
    AbandonShrink {
        vm: String,
        alias: String,
        immutable_target_bytes: u64,
        connection: String,
        apply: bool,
        attestation_path: String,
        command_timeout_seconds: u64,
        sample_interval_seconds: u64,
        convergence_timeout_seconds: u64,
    },
    ClearLatch {
        reason: String,
        config: PathBuf,
        vm: Option<String>,
        connection: String,
        apply: bool,
    },
}

pub fn parse_args(args: &[String]) -> Result<CliCommand, String> {
    let Some(mode) = args.first().map(String::as_str) else {
        return Err(usage().to_owned());
    };
    if mode == "--config" {
        return match args {
            [_, path] if !path.trim().is_empty() => Ok(CliCommand::Run {
                config: PathBuf::from(path),
            }),
            _ => Err(usage().to_owned()),
        };
    }
    if !matches!(
        mode,
        "status" | "attest" | "snapshot" | "validate" | "resize" | "abandon-shrink" | "clear-latch"
    ) {
        return Err(format!("unknown CLI command: {mode}\n{}", usage()));
    }
    if matches!(mode, "status" | "clear-latch") {
        let mut index = 1;
        let reason = if mode == "clear-latch" {
            index += 1;
            args.get(1)
                .filter(|reason| !reason.trim().is_empty())
                .ok_or_else(|| usage().to_owned())?
                .clone()
        } else {
            String::new()
        };
        let mut config = None;
        let mut vm = None;
        let mut connection = DEFAULT_CONNECTION.to_owned();
        let mut apply = false;
        while index < args.len() {
            match args[index].as_str() {
                "--apply" if mode == "clear-latch" && !apply => apply = true,
                "--vm" if vm.is_none() => {
                    index += 1;
                    vm = Some(
                        args.get(index)
                            .filter(|value| !value.trim().is_empty())
                            .ok_or_else(|| "--vm requires a VM name".to_owned())?
                            .clone(),
                    );
                }
                "--config" if config.is_none() => {
                    index += 1;
                    config = Some(PathBuf::from(
                        args.get(index)
                            .filter(|value| !value.trim().is_empty())
                            .ok_or_else(|| "--config requires a file path".to_owned())?,
                    ));
                }
                "--connect" => {
                    index += 1;
                    connection = args
                        .get(index)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| "--connect requires a libvirt URI".to_owned())?
                        .clone();
                }
                _ => return Err(format!("unknown CLI option: {}\n{}", args[index], usage())),
            }
            index += 1;
        }
        let config = required_option(config, "--config")?;
        return Ok(if mode == "status" {
            CliCommand::Status {
                config,
                vm,
                connection,
            }
        } else {
            CliCommand::ClearLatch {
                reason,
                config,
                vm,
                connection,
                apply,
            }
        });
    }
    let (minimum, maximum) = match mode {
        "resize" => (10, 13),
        "abandon-shrink" => (12, 15),
        "attest" => (6, 8),
        _ => (5, 7),
    };
    if args.len() < minimum || args.len() > maximum {
        return Err(usage().to_owned());
    }
    let vm = args[1].clone();
    let alias = args[2].clone();
    let mut connection = DEFAULT_CONNECTION.to_owned();
    let mut apply = false;
    let mut attestation_path = None;
    let mut host_min_headroom_bytes = None;
    let mut connection_supplied = false;
    let mut target_bytes = None;
    let mut command_timeout_seconds = None;
    let mut sample_interval_seconds = None;
    let mut convergence_timeout_seconds = None;
    let mut index = 3;
    let review_path = if mode == "attest" {
        index += 1;
        Some(args[3].clone())
    } else {
        None
    };
    if matches!(mode, "resize" | "abandon-shrink") {
        target_bytes = Some(
            args[index]
                .parse::<u64>()
                .ok()
                .ok_or_else(|| "resize target must be a nonnegative byte count".to_owned())?,
        );
        index += 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--apply" if matches!(mode, "resize" | "abandon-shrink") && !apply => apply = true,
            "--attestation" if matches!(mode, "resize" | "abandon-shrink") => {
                if attestation_path.is_some() {
                    return Err("--attestation may be supplied only once".to_owned());
                }
                index += 1;
                attestation_path = Some(
                    args.get(index)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| "--attestation requires a file path".to_owned())?
                        .clone(),
                );
            }
            "--host-min-headroom-bytes" if mode == "resize" => {
                if host_min_headroom_bytes.is_some() {
                    return Err("--host-min-headroom-bytes may be supplied only once".to_owned());
                }
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--host-min-headroom-bytes requires a value".to_owned())?;
                host_min_headroom_bytes = Some(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            "--host-min-headroom-bytes must be a positive byte count".to_owned()
                        })?,
                );
            }
            "--command-timeout-seconds" => {
                if command_timeout_seconds.is_some() {
                    return Err("--command-timeout-seconds may be supplied only once".to_owned());
                }
                index += 1;
                command_timeout_seconds = Some(positive_option(
                    args.get(index),
                    "--command-timeout-seconds",
                )?);
            }
            "--sample-interval-seconds" if mode == "abandon-shrink" => {
                if sample_interval_seconds.is_some() {
                    return Err("--sample-interval-seconds may be supplied only once".to_owned());
                }
                index += 1;
                sample_interval_seconds = Some(positive_option(
                    args.get(index),
                    "--sample-interval-seconds",
                )?);
            }
            "--convergence-timeout-seconds" if mode == "abandon-shrink" => {
                if convergence_timeout_seconds.is_some() {
                    return Err(
                        "--convergence-timeout-seconds may be supplied only once".to_owned()
                    );
                }
                index += 1;
                convergence_timeout_seconds = Some(positive_option(
                    args.get(index),
                    "--convergence-timeout-seconds",
                )?);
            }
            "--connect" => {
                if connection_supplied {
                    return Err("--connect may be supplied only once".to_owned());
                }
                connection_supplied = true;
                index += 1;
                connection = args
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "--connect requires a libvirt URI".to_owned())?
                    .clone();
            }
            _ => return Err(format!("unknown CLI option: {}\n{}", args[index], usage())),
        }
        index += 1;
    }
    for seconds in [
        command_timeout_seconds,
        sample_interval_seconds,
        convergence_timeout_seconds,
    ]
    .into_iter()
    .flatten()
    {
        let duration = Duration::from_secs(seconds);
        if std::time::Instant::now().checked_add(duration).is_none()
            || u64::try_from(duration.as_millis()).is_err()
        {
            return Err("CLI duration is too large".to_owned());
        }
    }
    if vm.trim().is_empty() {
        return Err("VM must be non-empty".to_owned());
    }
    if alias.is_empty()
        || !alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
    {
        return Err("ALIAS contains unsupported characters".to_owned());
    }
    let command = match mode {
        "snapshot" => CliCommand::Snapshot {
            vm,
            alias,
            connection,
            command_timeout_seconds: required_option(
                command_timeout_seconds,
                "--command-timeout-seconds",
            )?,
        },
        "validate" => CliCommand::Validate {
            vm,
            alias,
            connection,
            command_timeout_seconds: required_option(
                command_timeout_seconds,
                "--command-timeout-seconds",
            )?,
        },
        "attest" => CliCommand::Attest {
            vm,
            alias,
            review_path: review_path
                .ok_or_else(|| "attest review path was not supplied".to_owned())?,
            connection,
            command_timeout_seconds: required_option(
                command_timeout_seconds,
                "--command-timeout-seconds",
            )?,
        },
        "resize" => CliCommand::Resize {
            vm,
            alias,
            target_bytes: target_bytes
                .ok_or_else(|| "resize target was not supplied".to_owned())?,
            connection,
            apply,
            attestation_path: attestation_path
                .ok_or_else(|| "resize requires --attestation FILE".to_owned())?,
            host_min_headroom_bytes: host_min_headroom_bytes
                .ok_or_else(|| "resize requires --host-min-headroom-bytes BYTES".to_owned())?,
            command_timeout_seconds: required_option(
                command_timeout_seconds,
                "--command-timeout-seconds",
            )?,
        },
        "abandon-shrink" => CliCommand::AbandonShrink {
            vm,
            alias,
            immutable_target_bytes: target_bytes
                .ok_or_else(|| "immutable shrink target was not supplied".to_owned())?,
            connection,
            apply,
            attestation_path: attestation_path
                .ok_or_else(|| "abandon-shrink requires --attestation FILE".to_owned())?,
            command_timeout_seconds: required_option(
                command_timeout_seconds,
                "--command-timeout-seconds",
            )?,
            sample_interval_seconds: required_option(
                sample_interval_seconds,
                "--sample-interval-seconds",
            )?,
            convergence_timeout_seconds: required_option(
                convergence_timeout_seconds,
                "--convergence-timeout-seconds",
            )?,
        },
        _ => return Err(format!("unknown CLI command: {mode}")),
    };
    Ok(command)
}

fn positive_option(value: Option<&String>, name: &str) -> Result<u64, String> {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} requires a positive integer"))
}

fn required_option<T>(value: Option<T>, name: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{name} is required"))
}

pub fn usage() -> &'static str {
    "Usage: virtio-mem-host --config FILE\n       virtio-mem-host status --config FILE [--vm NAME] [--connect URI]\n       virtio-mem-host clear-latch REASON --config FILE [--vm NAME] [--apply] [--connect URI]\n       virtio-mem-host attest VM ALIAS REVIEW_FILE --command-timeout-seconds N [--connect URI]\n       virtio-mem-host [snapshot|validate] VM ALIAS --command-timeout-seconds N [--connect URI]\n       virtio-mem-host resize VM ALIAS TARGET_BYTES --attestation FILE --host-min-headroom-bytes BYTES --command-timeout-seconds N [--apply] [--connect URI]\n       virtio-mem-host abandon-shrink VM ALIAS IMMUTABLE_TARGET_BYTES --attestation FILE --command-timeout-seconds N --sample-interval-seconds N --convergence-timeout-seconds N [--apply] [--connect URI]"
}

pub fn run(command: CliCommand) -> Result<(), String> {
    let mut output = std::io::stdout().lock();
    run_with(command, ProcMeminfoSource::new(), &mut output)
}

fn run_with<H: HostMemorySource, W: Write>(
    command: CliCommand,
    host_memory: H,
    output: &mut W,
) -> Result<(), String> {
    match command {
        CliCommand::Run { .. } => Err("the controller runs from main, not the CLI".to_owned()),
        CliCommand::Status {
            config,
            vm,
            connection,
        } => run_configured_status(&config, vm.as_deref(), &connection, output),
        CliCommand::ClearLatch {
            reason,
            config,
            vm,
            connection,
            apply,
        } => {
            let config =
                HostConfig::load_vm(&config, vm.as_deref()).map_err(|error| error.to_string())?;
            let virsh = Virsh::with_connection(
                config.virsh_binary.clone(),
                config.command_timeout,
                connection,
            );
            let _owner = crate::ownership::DeviceOwner::for_domain(&virsh, &config.vm_name)?;
            AttestedCompatibilitySource::new(
                virsh.clone(),
                config.vm_name.clone(),
                config.alias.clone(),
                config.compatibility_attestation_path.clone(),
            )
            .compatibility()?
            .validate_for_resize()
            .map_err(|error| error.to_string())?;
            let live = VirshXmlSource::new(virsh, config.vm_name.clone(), config.alias.clone())
                .memory_state()?;
            let fingerprint = crate::attestation::read_attestation(Path::new(
                &config.compatibility_attestation_path,
            ))?
            .fingerprint_sha256;
            let message = clear_actuation_latch(
                &config,
                live,
                fingerprint,
                &reason,
                SystemUnixClock.now_unix_millis()?,
                apply,
            )?;
            writeln!(
                output,
                "mode={} {message}",
                if apply { "applied" } else { "dry_run" }
            )
            .map_err(|error| error.to_string())
        }
        CliCommand::Attest {
            vm,
            alias,
            review_path,
            connection,
            command_timeout_seconds,
        } => {
            let virsh = Virsh::with_connection(
                "virsh",
                Duration::from_secs(command_timeout_seconds),
                connection,
            );
            let evidence = VirshCompatibilityEvidenceSource::new(virsh, vm, alias).collect()?;
            let review = read_review(std::path::Path::new(&review_path))?;
            let document = CompatibilityAttestation::new(evidence, review)?;
            writeln!(output, "{}", document.to_pretty_json()?).map_err(|error| error.to_string())
        }
        CliCommand::Snapshot {
            vm,
            alias,
            connection,
            command_timeout_seconds,
        } => {
            let virsh = Virsh::with_connection(
                "virsh",
                Duration::from_secs(command_timeout_seconds),
                connection,
            );
            run_snapshot_with(virsh, &vm, &alias, output)
        }
        CliCommand::Validate {
            vm,
            alias,
            connection,
            command_timeout_seconds,
        } => {
            let virsh = Virsh::with_connection(
                "virsh",
                Duration::from_secs(command_timeout_seconds),
                connection,
            );
            run_validate_with(virsh, &vm, &alias, output)
        }
        CliCommand::Resize {
            vm,
            alias,
            target_bytes,
            connection,
            apply,
            attestation_path,
            host_min_headroom_bytes,
            command_timeout_seconds,
        } => {
            let virsh = Virsh::with_connection(
                "virsh",
                Duration::from_secs(command_timeout_seconds),
                &connection,
            );
            let _owner = crate::ownership::DeviceOwner::for_domain(&virsh, &vm)?;
            let compatibility_source = AttestedCompatibilitySource::new(
                virsh.clone(),
                vm.clone(),
                alias.clone(),
                attestation_path,
            );
            run_resize_with(
                virsh,
                ResizeOptions {
                    vm,
                    alias,
                    target_bytes,
                    connection,
                    apply,
                    host_min_headroom_bytes,
                },
                host_memory,
                compatibility_source,
                output,
            )
        }
        CliCommand::AbandonShrink {
            vm,
            alias,
            immutable_target_bytes,
            connection,
            apply,
            attestation_path,
            command_timeout_seconds,
            sample_interval_seconds,
            convergence_timeout_seconds,
        } => {
            let virsh = Virsh::with_connection(
                "virsh",
                Duration::from_secs(command_timeout_seconds),
                &connection,
            );
            let state_source = VirshXmlSource::new(virsh.clone(), vm.clone(), alias.clone());
            let _owner = crate::ownership::DeviceOwner::for_domain(&virsh, &vm)?;
            let compatibility_source = AttestedCompatibilitySource::new(
                virsh.clone(),
                vm.clone(),
                alias.clone(),
                attestation_path,
            );
            let sink = VirshResizeSink::new(virsh, vm, alias)
                .with_compatibility_source(compatibility_source);
            run_abandon_shrink_with(
                &state_source,
                &sink,
                AbandonOptions {
                    immutable_target_bytes,
                    connection,
                    apply,
                    sample_interval: Duration::from_secs(sample_interval_seconds),
                    convergence_timeout: Duration::from_secs(convergence_timeout_seconds),
                },
                std::thread::sleep,
                output,
            )
        }
    }
}

fn run_configured_status<W: Write>(
    config: &Path,
    vm: Option<&str>,
    connection: &str,
    output: &mut W,
) -> Result<(), String> {
    let config = HostConfig::load_vm(config, vm).map_err(|error| error.to_string())?;
    let virsh = Virsh::with_connection(
        config.virsh_binary.clone(),
        config.command_timeout,
        connection,
    );
    let live = VirshXmlSource::new(virsh.clone(), config.vm_name.clone(), config.alias.clone())
        .memory_state()?;
    let transport = match config.raw_telemetry_transport {
        RawTelemetryTransport::File => Transport::LocalFile,
        RawTelemetryTransport::QgaFile => {
            Transport::Qga(VirshGuestFiles::new(virsh, config.vm_name.clone()))
        }
    };
    let guest_input = match GuestChannel::new(
        transport,
        &config.raw_telemetry_path,
        &config.vm_name,
        &config.raw_telemetry_service_name,
        config.raw_telemetry_max_age,
        config.raw_telemetry_future_tolerance,
    )
    .read()
    {
        Ok(envelope) => GuestInputStatus {
            error: None,
            observed_unix_millis: Some(envelope.observed_unix_millis),
            request: envelope.request,
        },
        Err(error) => GuestInputStatus {
            error: Some(error),
            observed_unix_millis: None,
            request: None,
        },
    };
    let fingerprint =
        crate::attestation::read_attestation(Path::new(&config.compatibility_attestation_path))?
            .fingerprint_sha256;
    let status = read_controller_status(
        &config,
        live,
        guest_input,
        fingerprint,
        SystemUnixClock.now_unix_millis()?,
    )?;
    serde_json::to_writer_pretty(&mut *output, &status)
        .map_err(|error| format!("encode controller status: {error}"))?;
    writeln!(output).map_err(|error| error.to_string())
}

fn run_snapshot_with<C: VirshCommand, W: Write>(
    command: C,
    vm: &str,
    alias: &str,
    output: &mut W,
) -> Result<(), String> {
    let xml = command
        .run(&["dumpxml".to_owned(), vm.to_owned()])
        .map_err(|error| error.to_string())?;
    parse_virtio_mem_xml_for_alias(&xml, alias).map_err(|error| error.to_string())?;
    write!(output, "{xml}").map_err(|error| error.to_string())?;
    Ok(())
}

fn run_validate_with<C: VirshCommand, W: Write>(
    command: C,
    vm: &str,
    alias: &str,
    output: &mut W,
) -> Result<(), String> {
    let xml = command
        .run(&["dumpxml".to_owned(), vm.to_owned()])
        .map_err(|error| error.to_string())?;
    let snapshot =
        parse_virtio_mem_xml_for_alias(&xml, alias).map_err(|error| error.to_string())?;
    writeln!(output, "alias={}", snapshot.alias).map_err(|error| error.to_string())?;
    writeln!(output, "size_bytes={}", snapshot.memory.size_bytes)
        .map_err(|error| error.to_string())?;
    writeln!(
        output,
        "block_size_bytes={}",
        snapshot.memory.block_size_bytes
    )
    .map_err(|error| error.to_string())?;
    writeln!(
        output,
        "requested_bytes={}",
        snapshot.memory.requested_bytes
    )
    .map_err(|error| error.to_string())?;
    writeln!(output, "current_bytes={}", snapshot.memory.current_bytes)
        .map_err(|error| error.to_string())?;
    writeln!(output, "compatibility={:?}", snapshot.compatibility)
        .map_err(|error| error.to_string())?;
    Ok(())
}

struct ResizeOptions {
    vm: String,
    alias: String,
    target_bytes: u64,
    connection: String,
    apply: bool,
    host_min_headroom_bytes: u64,
}

fn run_resize_with<C: VirshCommand, H: HostMemorySource, E: CompatibilitySource, W: Write>(
    command: C,
    options: ResizeOptions,
    host_memory: H,
    compatibility_source: E,
    output: &mut W,
) -> Result<(), String> {
    let sink = VirshResizeSink::new(command, options.vm, options.alias)
        .with_compatibility_source(compatibility_source);
    let prepared = sink.prepare_resize(options.target_bytes)?;
    if prepared.target_bytes() > prepared.current_bytes() {
        validate_grow_headroom(
            prepared.current_bytes(),
            prepared.target_bytes(),
            host_memory.available_bytes()?,
            options.host_min_headroom_bytes,
        )?;
    }
    let mut full_arguments = vec!["virsh".to_owned(), "-c".to_owned(), options.connection];
    full_arguments.extend_from_slice(prepared.arguments());
    if options.apply {
        sink.apply_prepared(prepared)?;
        writeln!(
            output,
            "resize_applied target_bytes={}",
            options.target_bytes
        )
        .map_err(|error| error.to_string())?;
    } else {
        writeln!(output, "dry_run_argv={full_arguments:?}").map_err(|error| error.to_string())?;
    }
    Ok(())
}

struct AbandonOptions {
    immutable_target_bytes: u64,
    connection: String,
    apply: bool,
    sample_interval: Duration,
    convergence_timeout: Duration,
}

fn run_abandon_shrink_with<
    S: MemoryStateSource,
    C: VirshCommand,
    E: CompatibilitySource,
    F: FnMut(Duration),
    W: Write,
>(
    state_source: &S,
    sink: &VirshResizeSink<C, E>,
    options: AbandonOptions,
    mut sleep: F,
    output: &mut W,
) -> Result<(), String> {
    let first = state_source.memory_state()?;
    first.validate().map_err(|error| error.to_string())?;
    let mut recovery =
        AbandonToCurrent::new(options.immutable_target_bytes, first.block_size_bytes)?;
    let first_observation = shrink_observation(0, first);
    if recovery.observe(first_observation) != AbandonAction::Wait {
        return Err("first abandon-to-current sample is not a valid divergence".to_owned());
    }
    sleep(options.sample_interval);
    let second = state_source.memory_state()?;
    second.validate().map_err(|error| error.to_string())?;
    if second.size_bytes != first.size_bytes || second.block_size_bytes != first.block_size_bytes {
        return Err("virtio-mem geometry changed during recovery qualification".to_owned());
    }
    let stable_elapsed_millis = u64::try_from(options.sample_interval.as_millis())
        .map_err(|_| "sample interval is too large".to_owned())?;
    let stable_current_bytes =
        match recovery.observe(shrink_observation(stable_elapsed_millis, second)) {
            AbandonAction::Ready { target_bytes } => target_bytes,
            AbandonAction::Wait => {
                return Err("two unchanged samples did not qualify abandon-to-current".to_owned())
            }
            AbandonAction::Reject { reason } => return Err(reason),
        };
    let immediate = state_source.memory_state()?;
    immediate.validate().map_err(|error| error.to_string())?;
    recovery.verify_immediately_before_apply(shrink_observation(
        stable_elapsed_millis.saturating_add(1),
        immediate,
    ))?;

    let prepared = sink.prepare_abandon_to_current(options.immutable_target_bytes, immediate)?;
    let mut full_arguments = vec!["virsh".to_owned(), "-c".to_owned(), options.connection];
    full_arguments.extend_from_slice(prepared.arguments());
    if !options.apply {
        return writeln!(output, "dry_run_argv={full_arguments:?}")
            .map_err(|error| error.to_string());
    }

    let deadline = std::time::Instant::now()
        .checked_add(options.convergence_timeout)
        .ok_or_else(|| "convergence timeout is too large".to_owned())?;
    sink.apply_prepared(prepared)?;
    loop {
        if std::time::Instant::now() >= deadline {
            break;
        }
        let state = state_source.memory_state()?;
        state.validate().map_err(|error| error.to_string())?;
        if std::time::Instant::now() >= deadline {
            break;
        }
        if state.requested_bytes == stable_current_bytes
            && state.current_bytes == stable_current_bytes
        {
            return writeln!(
                output,
                "abandon_shrink_converged target_bytes={stable_current_bytes}"
            )
            .map_err(|error| error.to_string());
        }
        if state.requested_bytes != stable_current_bytes
            || state.current_bytes > stable_current_bytes
        {
            return Err("abandon-to-current entered an unexpected live state".to_owned());
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        let wait = options
            .sample_interval
            .min(deadline.saturating_duration_since(std::time::Instant::now()));
        sleep(wait);
    }
    Err(format!(
        "abandon-to-current did not converge within {:?}",
        options.convergence_timeout
    ))
}

fn shrink_observation(
    now_millis: u64,
    state: virtio_mem_core::VirtioMemState,
) -> ShrinkObservation {
    ShrinkObservation {
        now_millis,
        requested_bytes: state.requested_bytes,
        current_bytes: state.current_bytes,
        fresh: true,
        guest_running: true,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;
    use crate::compatibility_source::FixedCompatibilitySource;
    use crate::virsh::VirshError;
    use virtio_mem_core::VirtioMemCompatibility;

    const GIB: u64 = 1 << 30;
    const CONVERGED_XML: &str = "<domain><devices><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></devices></domain>";
    const SHRINK_PENDING_XML: &str = "<domain><devices><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>6</current></target><alias name='memory0'/></memory></devices></domain>";

    struct FakeHostMemory(u64);

    impl HostMemorySource for FakeHostMemory {
        fn available_bytes(&self) -> Result<u64, String> {
            Ok(self.0)
        }
    }

    struct FakeVirsh {
        calls: Rc<RefCell<Vec<Vec<String>>>>,
    }

    impl VirshCommand for FakeVirsh {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            self.calls.borrow_mut().push(arguments.to_vec());
            if arguments.first().map(String::as_str) == Some("dumpxml") {
                Ok(CONVERGED_XML.to_owned())
            } else {
                Ok(String::new())
            }
        }
    }

    struct RecoveryVirsh {
        calls: Rc<RefCell<Vec<Vec<String>>>>,
    }

    impl VirshCommand for RecoveryVirsh {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            self.calls.borrow_mut().push(arguments.to_vec());
            if arguments.first().map(String::as_str) == Some("dumpxml") {
                Ok(SHRINK_PENDING_XML.to_owned())
            } else {
                Ok(String::new())
            }
        }
    }

    struct ScriptedStates(RefCell<VecDeque<virtio_mem_core::VirtioMemState>>);

    impl MemoryStateSource for ScriptedStates {
        fn memory_state(&self) -> Result<virtio_mem_core::VirtioMemState, String> {
            self.0
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "scripted state sequence is exhausted".to_owned())
        }
    }

    fn state(requested_bytes: u64, current_bytes: u64) -> virtio_mem_core::VirtioMemState {
        virtio_mem_core::VirtioMemState {
            size_bytes: 8 * GIB,
            block_size_bytes: 2 * 1024 * 1024,
            requested_bytes,
            current_bytes,
        }
    }

    fn resize_options(apply: bool) -> ResizeOptions {
        ResizeOptions {
            vm: "guest".to_owned(),
            alias: "memory0".to_owned(),
            target_bytes: 6 * GIB,
            connection: DEFAULT_CONNECTION.to_owned(),
            apply,
            host_min_headroom_bytes: GIB,
        }
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_snapshot_and_resize_modes() {
        assert_eq!(
            parse_args(&args(&[
                "status",
                "--config",
                "/etc/virtio-mem-host/guest.toml",
                "--connect",
                "test:///default"
            ]))
            .expect("status"),
            CliCommand::Status {
                config: PathBuf::from("/etc/virtio-mem-host/guest.toml"),
                vm: None,
                connection: "test:///default".to_owned(),
            }
        );
        assert_eq!(
            parse_args(&args(&[
                "clear-latch",
                "reviewed recovery",
                "--connect",
                "test:///default",
                "--config",
                "guest.toml",
                "--vm",
                "guest",
            ]))
            .expect("clear-latch"),
            CliCommand::ClearLatch {
                reason: "reviewed recovery".to_owned(),
                config: PathBuf::from("guest.toml"),
                vm: Some("guest".to_owned()),
                connection: "test:///default".to_owned(),
                apply: false,
            }
        );
        assert_eq!(
            parse_args(&args(&[
                "abandon-shrink",
                "guest",
                "memory0",
                "4294967296",
                "--attestation",
                "reviewed.json",
                "--command-timeout-seconds",
                "4",
                "--sample-interval-seconds",
                "2",
                "--convergence-timeout-seconds",
                "8",
                "--apply",
            ]))
            .expect("abandon-shrink"),
            CliCommand::AbandonShrink {
                vm: "guest".to_owned(),
                alias: "memory0".to_owned(),
                immutable_target_bytes: 4 * GIB,
                connection: DEFAULT_CONNECTION.to_owned(),
                apply: true,
                attestation_path: "reviewed.json".to_owned(),
                command_timeout_seconds: 4,
                sample_interval_seconds: 2,
                convergence_timeout_seconds: 8,
            }
        );
        assert_eq!(
            parse_args(&args(&[
                "snapshot",
                "guest",
                "memory0",
                "--command-timeout-seconds",
                "4"
            ]))
            .expect("snapshot"),
            CliCommand::Snapshot {
                vm: "guest".to_owned(),
                alias: "memory0".to_owned(),
                connection: DEFAULT_CONNECTION.to_owned(),
                command_timeout_seconds: 4,
            }
        );
        assert!(matches!(
            parse_args(&args(&[
                "resize",
                "guest",
                "memory0",
                "2097152",
                "--attestation",
                "reviewed.json",
                "--host-min-headroom-bytes",
                "1073741824",
                "--command-timeout-seconds",
                "4",
                "--apply"
            ]))
            .expect("resize"),
            CliCommand::Resize { apply: true, .. }
        ));
        assert!(matches!(
            parse_args(&args(&[
                "attest",
                "guest",
                "memory0",
                "review.json",
                "--command-timeout-seconds",
                "4"
            ]))
            .expect("attest"),
            CliCommand::Attest { .. }
        ));
    }

    #[test]
    fn the_controller_runs_from_one_config_file() {
        assert_eq!(
            parse_args(&args(&["--config", "/etc/virtio-mem-host/guest.toml"])),
            Ok(CliCommand::Run {
                config: PathBuf::from("/etc/virtio-mem-host/guest.toml"),
            })
        );
    }

    #[test]
    fn rejects_invalid_cli_options_and_targets() {
        assert!(parse_args(&args(&["resize", "guest", "memory0", "0"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--connect"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--unknown"])).is_err());
        assert!(parse_args(&args(&["unknown"])).is_err());
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&args(&["--config"])).is_err());
        assert!(parse_args(&args(&["status"])).is_err());
        assert!(parse_args(&args(&["status", "--config", "a", "--apply"])).is_err());
        assert!(parse_args(&args(&[
            "status", "--config", "a", "--vm", "x", "--vm", "y"
        ]))
        .is_err());
        assert!(parse_args(&args(&["clear-latch", "reason", "--apply"])).is_err());
        assert!(parse_args(&args(&["validate", "guest", "bad/alias"])).is_err());
        assert!(parse_args(&args(&[
            "resize",
            "guest",
            "memory0",
            "6442450944",
            "--host-min-headroom-bytes",
            "1073741824"
        ]))
        .is_err());
        assert!(parse_args(&args(&["abandon-shrink", "guest", "memory0", "4294967296"])).is_err());
        assert!(parse_args(&args(&["qualify-shrink", "guest", "memory0", "4292870144"])).is_err());
    }

    #[test]
    fn abandon_shrink_qualifies_stability_rereads_and_converges_once() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let state_source = ScriptedStates(RefCell::new(VecDeque::from([
            state(4 * GIB, 6 * GIB),
            state(4 * GIB, 6 * GIB),
            state(4 * GIB, 6 * GIB),
            state(6 * GIB, 6 * GIB),
        ])));
        let sink = VirshResizeSink::new(
            RecoveryVirsh {
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());
        let mut waits = Vec::new();
        let mut output = Vec::new();

        run_abandon_shrink_with(
            &state_source,
            &sink,
            AbandonOptions {
                immutable_target_bytes: 4 * GIB,
                connection: DEFAULT_CONNECTION.to_owned(),
                apply: true,
                sample_interval: Duration::from_secs(2),
                convergence_timeout: Duration::from_secs(8),
            },
            |duration| waits.push(duration),
            &mut output,
        )
        .expect("qualified recovery converges");

        assert_eq!(waits, [Duration::from_secs(2)]);
        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(calls.borrow()[1][0], "update-memory-device");
        assert_eq!(
            String::from_utf8(output).expect("UTF-8 output"),
            "abandon_shrink_converged target_bytes=6442450944\n"
        );
    }

    #[test]
    fn abandon_shrink_rejects_moving_current_before_any_command() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let state_source = ScriptedStates(RefCell::new(VecDeque::from([
            state(4 * GIB, 6 * GIB),
            state(4 * GIB, 5 * GIB),
        ])));
        let sink = VirshResizeSink::new(
            RecoveryVirsh {
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());

        assert!(run_abandon_shrink_with(
            &state_source,
            &sink,
            AbandonOptions {
                immutable_target_bytes: 4 * GIB,
                connection: DEFAULT_CONNECTION.to_owned(),
                apply: true,
                sample_interval: Duration::from_secs(2),
                convergence_timeout: Duration::from_secs(8),
            },
            |_| {},
            &mut Vec::new(),
        )
        .is_err());
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn dry_run_reports_the_exact_vector_without_applying() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut output = Vec::new();
        run_resize_with(
            FakeVirsh {
                calls: Rc::clone(&calls),
            },
            resize_options(false),
            FakeHostMemory(4 * GIB),
            FixedCompatibilitySource::new(VirtioMemCompatibility::confirmed()),
            &mut output,
        )
        .expect("valid dry run");

        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(
            String::from_utf8(output).expect("UTF-8 output"),
            "dry_run_argv=[\"virsh\", \"-c\", \"qemu:///system\", \"update-memory-device\", \"guest\", \"--alias\", \"memory0\", \"--requested-size\", \"6291456\", \"--live\"]\n"
        );
    }

    #[test]
    fn snapshot_and_validate_are_read_only_and_alias_scoped() {
        let snapshot_calls = Rc::new(RefCell::new(Vec::new()));
        let mut snapshot_output = Vec::new();
        run_snapshot_with(
            FakeVirsh {
                calls: Rc::clone(&snapshot_calls),
            },
            "guest",
            "memory0",
            &mut snapshot_output,
        )
        .expect("valid snapshot");
        assert_eq!(snapshot_calls.borrow().len(), 1);
        assert_eq!(snapshot_calls.borrow()[0], ["dumpxml", "guest"]);
        assert_eq!(snapshot_output, CONVERGED_XML.as_bytes());

        let validate_calls = Rc::new(RefCell::new(Vec::new()));
        let mut validate_output = Vec::new();
        run_validate_with(
            FakeVirsh {
                calls: Rc::clone(&validate_calls),
            },
            "guest",
            "memory0",
            &mut validate_output,
        )
        .expect("valid validation");
        assert_eq!(validate_calls.borrow().len(), 1);
        let output = String::from_utf8(validate_output).expect("UTF-8 output");
        assert!(output.contains("alias=memory0"));
        assert!(output.contains("requested_bytes=4294967296"));
    }

    #[test]
    fn apply_sends_one_prepared_update_after_headroom_validation() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut output = Vec::new();
        run_resize_with(
            FakeVirsh {
                calls: Rc::clone(&calls),
            },
            resize_options(true),
            FakeHostMemory(4 * GIB),
            FixedCompatibilitySource::new(VirtioMemCompatibility::confirmed()),
            &mut output,
        )
        .expect("valid apply");

        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(calls.borrow()[1][0], "update-memory-device");
        assert_eq!(
            String::from_utf8(output).expect("UTF-8 output"),
            "resize_applied target_bytes=6442450944\n"
        );
    }

    #[test]
    fn insufficient_host_headroom_blocks_apply() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut output = Vec::new();
        let error = run_resize_with(
            FakeVirsh {
                calls: Rc::clone(&calls),
            },
            resize_options(true),
            FakeHostMemory(2 * GIB),
            FixedCompatibilitySource::new(VirtioMemCompatibility::confirmed()),
            &mut output,
        )
        .expect_err("reserve shortfall must fail closed");

        assert!(error.contains("host headroom is insufficient"));
        assert_eq!(calls.borrow().len(), 1);
        assert!(output.is_empty());
    }
}
