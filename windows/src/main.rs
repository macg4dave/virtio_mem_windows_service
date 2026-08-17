use std::process;

use virtio_mem_service::{
    install_service, stop_service, ServiceConfig, ServiceHost, StopSignal,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceCommand {
    Install,
    Run,
    Stop,
}

fn parse_command(args: &[String]) -> Option<ServiceCommand> {
    let command = args.first().map(String::as_str).unwrap_or("run");
    match command {
        "install" => Some(ServiceCommand::Install),
        "stop" => Some(ServiceCommand::Stop),
        "run" | "" => Some(ServiceCommand::Run),
        _ => Some(ServiceCommand::Run),
    }
}

fn run_service(config: ServiceConfig) -> Result<(), String> {
    config.validate().map_err(|error| error.to_string())?;

    let mut host = ServiceHost::new(|_stop: &StopSignal| Ok(()));
    host.run().map_err(|error| error.to_string())?;

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = parse_command(&args);

    let config = ServiceConfig::default();
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
        Some(ServiceCommand::Run) | None => {
            if let Err(error) = run_service(config) {
                eprintln!("virtio-mem service startup failed: {error}");
                process::exit(1);
            }
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
    }
}
