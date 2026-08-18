use std::process::ExitCode;
use std::sync::{atomic::AtomicBool, Arc};

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use virtio_mem_host::config::HostConfig;
use virtio_mem_host::qga::VirshGuestAgent;
use virtio_mem_host::resize_sink::VirshResizeSink;
use virtio_mem_host::runtime::HostRuntime;
use virtio_mem_host::virsh::Virsh;
use virtio_mem_host::xml_source::VirshXmlSource;

fn main() -> ExitCode {
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
    let runtime = HostRuntime::new(
        VirshGuestAgent::new(virsh.clone(), config.vm_name.clone()),
        VirshXmlSource::new(virsh.clone(), config.vm_name.clone(), config.alias.clone()),
        VirshResizeSink::new(virsh, config.vm_name.clone(), config.alias.clone()),
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
