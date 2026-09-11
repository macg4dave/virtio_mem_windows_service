use std::process::ExitCode;
use std::sync::{atomic::AtomicBool, Arc};

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use virtio_mem_host::attestation::AttestedCompatibilitySource;
use virtio_mem_host::config::{DemandSourceMode, HostConfig, RawTelemetryTransport, StatsSource};
use virtio_mem_host::dommemstat::DomMemStatSource;
use virtio_mem_host::host_memory::ProcMeminfoSource;
use virtio_mem_host::qga::{VirshGuestAgent, VirshQgaFileReader};
use virtio_mem_host::raw_telemetry::{
    FileRawTelemetrySource, QgaFileRawTelemetrySource, RawTelemetrySource,
};
use virtio_mem_host::resize_sink::VirshResizeSink;
use virtio_mem_host::runtime::{DemandSource, GuestStatsDemandSource, HostRuntime};
use virtio_mem_host::target_policy::TargetDemandSource;
use virtio_mem_host::virsh::Virsh;
use virtio_mem_host::xml_source::VirshXmlSource;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match virtio_mem_host::cli::parse_args(&args) {
        Ok(Some(command)) => {
            return match virtio_mem_host::cli::run(command) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("virtio-mem-host CLI failed: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
        Ok(None) => {}
    }
    let config = match HostConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("virtio-mem-host configuration error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    if let Err(error) = flag::register(SIGTERM, Arc::clone(&stop))
        .and_then(|_| flag::register(SIGINT, Arc::clone(&stop)))
    {
        eprintln!("virtio-mem-host signal setup error: {error}");
        return ExitCode::FAILURE;
    }
    match config.demand_source {
        DemandSourceMode::Raw => {
            let raw: Box<dyn RawTelemetrySource> = match config.raw_telemetry_transport {
                RawTelemetryTransport::File => Box::new(
                    FileRawTelemetrySource::new(
                        &config.raw_telemetry_path,
                        &config.vm_name,
                        &config.raw_telemetry_service_name,
                        config.raw_telemetry_max_age,
                        config.raw_telemetry_future_tolerance,
                    )
                    .with_replay_state_path(&config.raw_telemetry_ack_path),
                ),
                RawTelemetryTransport::QgaFile => {
                    let virsh = Virsh::new(config.virsh_binary.clone(), config.command_timeout);
                    Box::new(QgaFileRawTelemetrySource::new(
                        VirshQgaFileReader::new(virsh, config.vm_name.clone()),
                        &config.raw_telemetry_path,
                        &config.raw_telemetry_ack_path,
                        &config.vm_name,
                        &config.raw_telemetry_service_name,
                        config.raw_telemetry_max_age,
                        config.raw_telemetry_future_tolerance,
                    ))
                }
            };
            match TargetDemandSource::new(raw, &config) {
                Ok(source) => run_controller(source, config, &stop),
                Err(error) => {
                    eprintln!("virtio-mem-host target policy error: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        DemandSourceMode::GuestStats => {
            let virsh = Virsh::new(config.virsh_binary.clone(), config.command_timeout);
            match config.stats_source {
                StatsSource::DomMemStat => run_controller(
                    GuestStatsDemandSource::new(DomMemStatSource::new(
                        virsh,
                        config.vm_name.clone(),
                        config.stats_max_age,
                        config.stats_future_tolerance,
                    )),
                    config,
                    &stop,
                ),
                StatsSource::Qga => run_controller(
                    GuestStatsDemandSource::new(VirshGuestAgent::new(
                        virsh,
                        config.vm_name.clone(),
                    )),
                    config,
                    &stop,
                ),
            }
        }
    }
}

fn run_controller<D: DemandSource>(
    demand_source: D,
    config: HostConfig,
    stop: &AtomicBool,
) -> ExitCode {
    let virsh = Virsh::new(config.virsh_binary.clone(), config.command_timeout);
    let runtime = HostRuntime::new(
        demand_source,
        VirshXmlSource::new(virsh.clone(), config.vm_name.clone(), config.alias.clone()),
        VirshResizeSink::new(virsh.clone(), config.vm_name.clone(), config.alias.clone())
            .with_compatibility_source(AttestedCompatibilitySource::new(
                virsh,
                config.vm_name.clone(),
                config.alias.clone(),
                config.compatibility_attestation_path.clone(),
            )),
        ProcMeminfoSource::new(),
        config,
    );
    match runtime.run(stop) {
        Ok(()) => {
            eprintln!("virtio-mem-host stopped");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("virtio-mem-host failed: {error}");
            ExitCode::FAILURE
        }
    }
}
