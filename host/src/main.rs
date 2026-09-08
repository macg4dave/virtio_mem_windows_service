use std::process::ExitCode;
use std::sync::{atomic::AtomicBool, Arc};

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use virtio_mem_host::attestation::AttestedCompatibilitySource;
use virtio_mem_host::config::HostConfig;
use virtio_mem_host::host_memory::ProcMeminfoSource;
use virtio_mem_host::raw_telemetry::FileRawTelemetrySource;
use virtio_mem_host::resize_sink::VirshResizeSink;
use virtio_mem_host::runtime::HostRuntime;
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
    let virsh = Virsh::new(config.virsh_binary.clone(), config.command_timeout);
    let raw_telemetry = FileRawTelemetrySource::new(
        &config.raw_telemetry_path,
        &config.vm_name,
        &config.raw_telemetry_service_name,
        config.raw_telemetry_max_age,
        config.raw_telemetry_future_tolerance,
    );
    let runtime = HostRuntime::new(
        raw_telemetry,
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
    match runtime.run(&stop) {
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
