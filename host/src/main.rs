use std::path::Path;
use std::process::ExitCode;
use std::sync::{atomic::AtomicBool, Arc};

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use virtio_mem_host::attestation::{read_attestation, AttestedCompatibilitySource};
use virtio_mem_host::cli::CliCommand;
use virtio_mem_host::config::{HostConfig, HostSettings, RawTelemetryTransport};
use virtio_mem_host::guest_channel::{GuestChannel, Transport};
use virtio_mem_host::host_memory::ProcMeminfoSource;
use virtio_mem_host::ownership::DeviceOwner;
use virtio_mem_host::qga::VirshGuestFiles;
use virtio_mem_host::resize_sink::VirshResizeSink;
use virtio_mem_host::runtime::{HostRuntime, VmController};
use virtio_mem_host::virsh::Virsh;
use virtio_mem_host::xml_source::VirshXmlSource;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config_path = match virtio_mem_host::cli::parse_args(&args) {
        Ok(CliCommand::Run { config }) => config,
        Ok(command) => {
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
    };
    let settings = match HostSettings::load(&config_path) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("virtio-mem-host configuration error: {error}");
            return ExitCode::FAILURE;
        }
    };
    // virsh binary and timeouts are global settings, shared by every VM.
    let first = &settings.vms[0];
    let virsh = Virsh::new(first.virsh_binary.clone(), first.command_timeout);
    if let Err(error) = virsh.require_management_access() {
        eprintln!("virtio-mem-host authorization error: {error}");
        return ExitCode::FAILURE;
    }
    let stop = Arc::new(AtomicBool::new(false));
    if let Err(error) = flag::register(SIGTERM, Arc::clone(&stop))
        .and_then(|_| flag::register(SIGINT, Arc::clone(&stop)))
    {
        eprintln!("virtio-mem-host signal setup error: {error}");
        return ExitCode::FAILURE;
    }
    // Device owners are held until the runtime ends.
    let mut owners = Vec::with_capacity(settings.vms.len());
    let mut vms = Vec::with_capacity(settings.vms.len());
    for config in settings.vms {
        match vm_controller(&virsh, config) {
            Ok((owner, vm)) => {
                owners.push(owner);
                vms.push(vm);
            }
            Err(error) => {
                eprintln!("virtio-mem-host controller setup error: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    let mut runtime = match HostRuntime::new(vms, ProcMeminfoSource::new(), settings.pool_bytes) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("virtio-mem-host configuration error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.run(&stop);
    drop(owners);
    match result {
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

type Controller = VmController<
    GuestChannel<VirshGuestFiles<Virsh>>,
    VirshXmlSource<Virsh>,
    VirshResizeSink<Virsh, AttestedCompatibilitySource<Virsh>>,
>;

fn vm_controller(virsh: &Virsh, config: HostConfig) -> Result<(DeviceOwner, Controller), String> {
    let transport = match config.raw_telemetry_transport {
        RawTelemetryTransport::File => Transport::LocalFile,
        RawTelemetryTransport::QgaFile => {
            Transport::Qga(VirshGuestFiles::new(virsh.clone(), config.vm_name.clone()))
        }
    };
    let owner = DeviceOwner::for_domain(virsh, &config.vm_name)?;
    let fingerprint =
        read_attestation(Path::new(&config.compatibility_attestation_path))?.fingerprint_sha256;
    let controller = VmController::new(
        GuestChannel::new(
            transport,
            &config.raw_telemetry_path,
            &config.vm_name,
            &config.raw_telemetry_service_name,
            config.raw_telemetry_max_age,
            config.raw_telemetry_future_tolerance,
        ),
        VirshXmlSource::new(virsh.clone(), config.vm_name.clone(), config.alias.clone()),
        VirshResizeSink::new(virsh.clone(), config.vm_name.clone(), config.alias.clone())
            .with_compatibility_source(AttestedCompatibilitySource::new(
                virsh.clone(),
                config.vm_name.clone(),
                config.alias.clone(),
                config.compatibility_attestation_path.clone(),
            )),
        config,
        fingerprint,
    )?;
    Ok((owner, controller))
}
