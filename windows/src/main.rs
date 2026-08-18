use std::process;

use virtio_mem_service::{
    install_service, remove_service, run_as_service, start_service, stop_service,
    NamedPipeGuestAgent, QgaPollingWorker, ServiceConfig, ServiceHost,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceCommand {
    Install,
    Run,
    Stop,
    Start,
    Remove,
    Help,
}

fn parse_command(args: &[String]) -> Option<ServiceCommand> {
    let command = args.first().map(String::as_str).unwrap_or("run");
    match command {
        "install" => Some(ServiceCommand::Install),
        "stop" => Some(ServiceCommand::Stop),
        "start" => Some(ServiceCommand::Start),
        "remove" | "delete" => Some(ServiceCommand::Remove),
        "help" | "--help" | "-h" => Some(ServiceCommand::Help),
        "run" | "" => Some(ServiceCommand::Run),
        _ => None,
    }
}

fn run_service(config: ServiceConfig) -> Result<(), String> {
    config.validate().map_err(|error| error.to_string())?;
    let worker = QgaPollingWorker::new(
        NamedPipeGuestAgent::with_operation_timeout(
            config.qga_pipe_path.clone(),
            config.qga_operation_timeout,
        ),
        config.poll_interval,
    )?;

    let mut host = ServiceHost::with_shutdown_timeout(worker, config.shutdown_timeout);
    host.run().map_err(|error| error.to_string())?;

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = parse_command(&args);

    let config = match ServiceConfig::load_default() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("service configuration failed: {error}");
            process::exit(1);
        }
    };
    if matches!(command, Some(ServiceCommand::Run)) {
        match run_as_service() {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                eprintln!("service dispatcher failed: {error}");
                process::exit(1);
            }
        }
    }
    match command {
        Some(ServiceCommand::Install) => {
            if let Err(error) = install_service(&config) {
                eprintln!("service installation failed: {error}");
                process::exit(1);
            }
            println!("service installed successfully");
        }
        Some(ServiceCommand::Stop) => {
            if let Err(error) = stop_service(&config.service_name) {
                eprintln!("service stop failed: {error}");
                process::exit(1);
            }
            println!("service stop requested");
        }
        Some(ServiceCommand::Start) => {
            if let Err(error) = start_service(&config.service_name) {
                eprintln!("service start failed: {error}");
                process::exit(1);
            }
            println!("service start requested");
        }
        Some(ServiceCommand::Remove) => {
            if let Err(error) = remove_service(&config.service_name) {
                eprintln!("service removal failed: {error}");
                process::exit(1);
            }
            println!("service removed successfully");
        }
        Some(ServiceCommand::Run) | None => {
            if command.is_none() {
                eprintln!("unknown command; use 'help' for usage");
                process::exit(2);
            }
            if let Err(error) = run_service(config) {
                eprintln!("virtio-mem service startup failed: {error}");
                process::exit(1);
            }
        }
        Some(ServiceCommand::Help) => {
            println!("Usage: virtio-mem-service [install|start|run|stop|remove|help]");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_service_configuration_before_startup() {
        let config = ServiceConfig {
            service_name: String::new(),
            ..ServiceConfig::default()
        };

        assert!(run_service(config).is_err());
    }

    #[test]
    fn parses_service_commands_for_install_and_stop() {
        assert_eq!(
            parse_command(&[String::from("install")]),
            Some(ServiceCommand::Install)
        );
        assert_eq!(
            parse_command(&[String::from("stop")]),
            Some(ServiceCommand::Stop)
        );
        assert_eq!(
            parse_command(&[String::from("run")]),
            Some(ServiceCommand::Run)
        );
        assert_eq!(parse_command(&[]), Some(ServiceCommand::Run));
        assert_eq!(
            parse_command(&[String::from("start")]),
            Some(ServiceCommand::Start)
        );
        assert_eq!(
            parse_command(&[String::from("remove")]),
            Some(ServiceCommand::Remove)
        );
        assert_eq!(
            parse_command(&[String::from("help")]),
            Some(ServiceCommand::Help)
        );
        assert_eq!(parse_command(&[String::from("unknown")]), None);
    }
}
