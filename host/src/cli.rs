use std::io::Write;
use std::time::Duration;

use virtio_mem_core::{
    parse_behavior_evidence, parse_virtio_mem_xml_for_alias, AbandonAction, AbandonToCurrent,
    ResizeDecision, ShrinkObservation,
};

use crate::attestation::{
    read_review, AttestedCompatibilitySource, CompatibilityAttestation,
    VirshCompatibilityEvidenceSource,
};
use crate::compatibility_source::CompatibilitySource;
use crate::config::{HostConfig, StatsSource};
use crate::dommemstat::DomMemStatSource;
use crate::host_memory::{validate_grow_headroom, HostMemorySource, ProcMeminfoSource};
use crate::qga::VirshGuestAgent;
use crate::resize_sink::VirshResizeSink;
use crate::runtime::{evaluate_memory_decision, GuestStatsSource, MemoryStateSource};
use crate::target_policy::clear_actuation_latch;
use crate::virsh::{Virsh, VirshCommand};
use crate::xml_source::VirshXmlSource;

const DEFAULT_CONNECTION: &str = "qemu:///system";

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Decision {
        connection: String,
    },
    Evidence {
        path: String,
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
        connection: String,
        apply: bool,
    },
}

pub fn parse_args(args: &[String]) -> Result<Option<CliCommand>, String> {
    let Some(mode) = args.first().map(String::as_str) else {
        return Ok(None);
    };
    if !matches!(
        mode,
        "decision"
            | "evidence"
            | "attest"
            | "snapshot"
            | "validate"
            | "resize"
            | "abandon-shrink"
            | "clear-latch"
    ) {
        return Err(format!("unknown CLI command: {mode}\n{}", usage()));
    }
    if mode == "evidence" {
        if args.len() != 2 || args[1].trim().is_empty() {
            return Err(usage().to_owned());
        }
        return Ok(Some(CliCommand::Evidence {
            path: args[1].clone(),
        }));
    }
    if mode == "decision" {
        let connection = match args {
            [_] => DEFAULT_CONNECTION.to_owned(),
            [_, option, value] if option == "--connect" && !value.is_empty() => value.clone(),
            _ => return Err(usage().to_owned()),
        };
        return Ok(Some(CliCommand::Decision { connection }));
    }
    if mode == "clear-latch" {
        if args.len() < 2 || args.len() > 5 || args[1].trim().is_empty() {
            return Err(usage().to_owned());
        }
        let reason = args[1].clone();
        let mut connection = DEFAULT_CONNECTION.to_owned();
        let mut apply = false;
        let mut index = 2;
        while index < args.len() {
            match args[index].as_str() {
                "--apply" if !apply => apply = true,
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
        return Ok(Some(CliCommand::ClearLatch {
            reason,
            connection,
            apply,
        }));
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
                .filter(|value| *value > 0)
                .ok_or_else(|| "resize target must be a positive byte count".to_owned())?,
        );
        index += 1;
    }
    while index < args.len() {
        match args[index].as_str() {
            "--apply"
                if matches!(mode, "resize" | "abandon-shrink") && !apply =>
            {
                apply = true
            }
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
                        "--convergence-timeout-seconds may be supplied only once".to_owned(),
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
    Ok(Some(command))
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
    "Usage: virtio-mem-host decision [--connect URI]\n       virtio-mem-host clear-latch REASON [--apply] [--connect URI]\n       virtio-mem-host evidence FILE\n       virtio-mem-host attest VM ALIAS REVIEW_FILE --command-timeout-seconds N [--connect URI]\n       virtio-mem-host [snapshot|validate] VM ALIAS --command-timeout-seconds N [--connect URI]\n       virtio-mem-host resize VM ALIAS TARGET_BYTES --attestation FILE --host-min-headroom-bytes BYTES --command-timeout-seconds N [--apply] [--connect URI]\n       virtio-mem-host abandon-shrink VM ALIAS IMMUTABLE_TARGET_BYTES --attestation FILE --command-timeout-seconds N --sample-interval-seconds N --convergence-timeout-seconds N [--apply] [--connect URI]"
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
        CliCommand::Decision { connection } => run_configured_decision(&connection, output),
        CliCommand::ClearLatch {
            reason,
            connection,
            apply,
        } => {
            let config = HostConfig::from_env().map_err(|error| error.to_string())?;
            let virsh = Virsh::with_connection(
                config.virsh_binary.clone(),
                config.command_timeout,
                connection,
            );
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
            let message = clear_actuation_latch(&config, live, &reason, apply)?;
            writeln!(
                output,
                "mode={} {message}",
                if apply { "applied" } else { "dry_run" }
            )
            .map_err(|error| error.to_string())
        }
        CliCommand::Evidence { path } => {
            let json = std::fs::read_to_string(&path)
                .map_err(|error| format!("failed to read evidence file {path}: {error}"))?;
            run_evidence_with(&json, output)
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

fn run_configured_decision<W: Write>(connection: &str, output: &mut W) -> Result<(), String> {
    let config = HostConfig::from_env().map_err(|error| error.to_string())?;
    let virsh = Virsh::with_connection(
        config.virsh_binary.clone(),
        config.command_timeout,
        connection,
    );
    let state_source =
        VirshXmlSource::new(virsh.clone(), config.vm_name.clone(), config.alias.clone());
    match config.stats_source {
        StatsSource::DomMemStat => run_decision_with(
            DomMemStatSource::new(
                virsh,
                config.vm_name.clone(),
                config.stats_max_age,
                config.stats_future_tolerance,
            ),
            state_source,
            &config,
            "dommemstat",
            output,
        ),
        StatsSource::Qga => run_decision_with(
            VirshGuestAgent::new(virsh, config.vm_name.clone()),
            state_source,
            &config,
            "qga",
            output,
        ),
    }
}

fn run_decision_with<G: GuestStatsSource, S: MemoryStateSource, W: Write>(
    stats_source: G,
    state_source: S,
    config: &HostConfig,
    source_name: &str,
    output: &mut W,
) -> Result<(), String> {
    let state = state_source.memory_state()?;
    state.validate().map_err(|error| error.to_string())?;
    let stats = stats_source.get_memory_stats()?;
    let decision = evaluate_memory_decision(&stats, state, config)?;
    writeln!(output, "source={source_name}").map_err(|error| error.to_string())?;
    writeln!(output, "free_bytes={}", stats.free_bytes).map_err(|error| error.to_string())?;
    writeln!(output, "available_bytes={}", stats.available_bytes)
        .map_err(|error| error.to_string())?;
    writeln!(output, "requested_bytes={}", state.requested_bytes)
        .map_err(|error| error.to_string())?;
    writeln!(output, "current_bytes={}", state.current_bytes).map_err(|error| error.to_string())?;
    match decision {
        ResizeDecision::NoChange => writeln!(output, "decision=no_change"),
        ResizeDecision::WaitForConvergence => writeln!(output, "decision=wait_for_convergence"),
        ResizeDecision::Request { requested_bytes } => {
            writeln!(output, "decision=request target_bytes={requested_bytes}")
        }
    }
    .map_err(|error| error.to_string())
}

fn run_evidence_with<W: Write>(json: &str, output: &mut W) -> Result<(), String> {
    let document = parse_behavior_evidence(json).map_err(|error| error.to_string())?;
    writeln!(output, "evidence_valid version={}", document.version)
        .map_err(|error| error.to_string())?;
    writeln!(output, "operation_id={}", document.identity.operation_id)
        .map_err(|error| error.to_string())?;
    writeln!(output, "vm_name={}", document.identity.vm_name).map_err(|error| error.to_string())?;
    writeln!(output, "device_alias={}", document.identity.device_alias)
        .map_err(|error| error.to_string())?;
    writeln!(output, "samples={}", document.samples.len()).map_err(|error| error.to_string())
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
    let stable_current_bytes = match recovery.observe(shrink_observation(
        stable_elapsed_millis,
        second,
    )) {
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

    let prepared =
        sink.prepare_abandon_to_current(options.immutable_target_bytes, stable_current_bytes)?;
    let mut full_arguments = vec!["virsh".to_owned(), "-c".to_owned(), options.connection];
    full_arguments.extend_from_slice(prepared.arguments());
    if !options.apply {
        return writeln!(output, "dry_run_argv={full_arguments:?}")
            .map_err(|error| error.to_string());
    }

    sink.apply_prepared(prepared)?;
    let mut elapsed = Duration::ZERO;
    loop {
        let state = state_source.memory_state()?;
        state.validate().map_err(|error| error.to_string())?;
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
        if elapsed >= options.convergence_timeout {
            break;
        }
        let wait = options
            .sample_interval
            .min(options.convergence_timeout.saturating_sub(elapsed));
        sleep(wait);
        elapsed += wait;
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
    use virtio_mem_core::{MemoryStats, VirtioMemCompatibility, VirtioMemState};

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
                "clear-latch",
                "reviewed recovery",
                "--connect",
                "test:///default",
            ]))
            .expect("clear-latch"),
            Some(CliCommand::ClearLatch {
                reason: "reviewed recovery".to_owned(),
                connection: "test:///default".to_owned(),
                apply: false,
            })
        );
        assert_eq!(
            parse_args(&args(&["decision"])).expect("decision"),
            Some(CliCommand::Decision {
                connection: DEFAULT_CONNECTION.to_owned()
            })
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
            Some(CliCommand::AbandonShrink {
                vm: "guest".to_owned(),
                alias: "memory0".to_owned(),
                immutable_target_bytes: 4 * GIB,
                connection: DEFAULT_CONNECTION.to_owned(),
                apply: true,
                attestation_path: "reviewed.json".to_owned(),
                command_timeout_seconds: 4,
                sample_interval_seconds: 2,
                convergence_timeout_seconds: 8,
            })
        );
        assert_eq!(
            parse_args(&args(&["decision", "--connect", "test:///default"]))
                .expect("decision connection"),
            Some(CliCommand::Decision {
                connection: "test:///default".to_owned()
            })
        );
        assert_eq!(
            parse_args(&args(&["evidence", "capture.json"])).expect("evidence"),
            Some(CliCommand::Evidence {
                path: "capture.json".to_owned(),
            })
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
            Some(CliCommand::Snapshot {
                vm: "guest".to_owned(),
                alias: "memory0".to_owned(),
                connection: DEFAULT_CONNECTION.to_owned(),
                command_timeout_seconds: 4,
            })
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
            Some(CliCommand::Resize { apply: true, .. })
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
            Some(CliCommand::Attest { .. })
        ));
    }

    #[test]
    fn rejects_invalid_cli_options_and_targets() {
        assert!(parse_args(&args(&["resize", "guest", "memory0", "0"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--connect"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--unknown"])).is_err());
        assert!(parse_args(&args(&["unknown"])).is_err());
        assert!(parse_args(&args(&["validate", "guest", "bad/alias"])).is_err());
        assert!(parse_args(&args(&["evidence"])).is_err());
        assert!(parse_args(&args(&["evidence", "capture.json", "extra"])).is_err());
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
    fn validates_correlated_evidence_without_live_commands() {
        let identity =
            r#"{"operation_id":"op-1","vm_name":"win11_gpu","device_alias":"ua-virtiomem0"}"#;
        let json = format!(
            r#"{{"version":1,"identity":{identity},"samples":[
                {{"identity":{identity},"sequence":1,"wall_clock_unix_millis":1000,"monotonic_millis":10,"source_id":"libvirt:qemu:///system","unit":"bytes","layer":"host_libvirt","phase":"before","size_bytes":8589934592,"block_size_bytes":2097152,"requested_bytes":1073741824,"current_bytes":1073741824}},
                {{"identity":{identity},"sequence":2,"wall_clock_unix_millis":1001,"monotonic_millis":20,"source_id":"windows-scm:ice101","unit":"bytes","layer":"windows_health","viomem_running":true}},
                {{"identity":{identity},"sequence":3,"wall_clock_unix_millis":1002,"monotonic_millis":30,"source_id":"systemd:rhel-host","unit":"bytes","layer":"controller_state","enabled":true,"active":false}},
                {{"identity":{identity},"sequence":4,"wall_clock_unix_millis":1003,"monotonic_millis":40,"source_id":"libvirt:qemu:///system","unit":"bytes","layer":"host_libvirt","phase":"after","size_bytes":8589934592,"block_size_bytes":2097152,"requested_bytes":1075838976,"current_bytes":1075838976}}
            ]}}"#
        );
        let mut output = Vec::new();
        run_evidence_with(&json, &mut output).expect("valid evidence");
        assert_eq!(
            String::from_utf8(output).expect("UTF-8 output"),
            "evidence_valid version=1\noperation_id=op-1\nvm_name=win11_gpu\ndevice_alias=ua-virtiomem0\nsamples=4\n"
        );

        let mixed = json.replacen("\"operation_id\":\"op-1\"", "\"operation_id\":\"other\"", 1);
        assert!(run_evidence_with(&mixed, &mut Vec::new()).is_err());
    }

    struct FixedStats(MemoryStats);

    impl GuestStatsSource for FixedStats {
        fn get_memory_stats(&self) -> Result<MemoryStats, String> {
            Ok(MemoryStats {
                free_bytes: self.0.free_bytes,
                available_bytes: self.0.available_bytes,
                total_bytes: self.0.total_bytes,
            })
        }
    }

    struct FixedState(VirtioMemState);

    impl MemoryStateSource for FixedState {
        fn memory_state(&self) -> Result<VirtioMemState, String> {
            Ok(self.0)
        }
    }

    #[test]
    fn decision_preview_uses_the_runtime_evaluator_without_actuation() {
        let config = HostConfig {
            vm_name: "guest".to_owned(),
            alias: "memory0".to_owned(),
            min_memory_bytes: 4 * GIB,
            max_memory_bytes: 8 * GIB,
            lower_threshold_bytes: GIB,
            upper_threshold_bytes: 3 * GIB,
            grow_step_bytes: GIB,
            shrink_step_bytes: 64 * 1024 * 1024,
            fixed_visible_base_bytes: 4 * GIB,
            physical_reserve_bytes: 2 * GIB,
            commit_reserve_bytes: 2 * GIB,
            safe_floor_physical_reserve_bytes: GIB,
            safe_floor_commit_reserve_bytes: GIB,
            reclaim_history: Duration::from_secs(600),
            reclaim_max_gap: Duration::from_secs(60),
            downward_hysteresis_bytes: 256 * 1024 * 1024,
            policy_state_path: "state.json".to_owned(),
            poll_interval: Duration::from_secs(30),
            command_timeout: Duration::from_secs(10),
            convergence_timeout: Duration::from_secs(300),
            virsh_binary: "virsh".to_owned(),
            stats_source: StatsSource::DomMemStat,
            demand_source: crate::config::DemandSourceMode::Raw,
            stats_max_age: Duration::from_secs(60),
            stats_future_tolerance: Duration::from_secs(5),
            raw_telemetry_path: "guest.telemetry.jsonl".to_owned(),
            raw_telemetry_service_name: "VirtioMemService".to_owned(),
            raw_telemetry_max_age: Duration::from_secs(60),
            raw_telemetry_future_tolerance: Duration::from_secs(5),
            host_min_headroom_bytes: 4 * GIB,
            compatibility_attestation_path: "reviewed.json".to_owned(),
            automatic_windows_shrink: false,
            shrink_renotification: false,
            shrink_retry_delays: Vec::new(),
        };
        let mut output = Vec::new();
        run_decision_with(
            FixedStats(MemoryStats {
                free_bytes: GIB / 2,
                available_bytes: 6 * GIB,
                total_bytes: 6 * GIB,
            }),
            FixedState(VirtioMemState {
                size_bytes: 10 * GIB,
                block_size_bytes: 2 * 1024 * 1024,
                requested_bytes: 4 * GIB,
                current_bytes: 4 * GIB,
            }),
            &config,
            "dommemstat",
            &mut output,
        )
        .expect("valid preview");
        assert_eq!(
            String::from_utf8(output).expect("UTF-8 output"),
            "source=dommemstat\nfree_bytes=536870912\navailable_bytes=6442450944\nrequested_bytes=4294967296\ncurrent_bytes=4294967296\ndecision=request target_bytes=5368709120\n"
        );
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
