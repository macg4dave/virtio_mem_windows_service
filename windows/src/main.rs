use std::process;

use virtio_mem_service::{
    install_service, process_session_id, remove_service, run_as_service, start_service,
    stop_service, AtomicRawTelemetryPublisher, NativeMemoryTelemetry, RawTelemetryWorker,
    RuntimeWiringError, ServiceConfig, ServiceHost, SystemTelemetryClock,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupRoute {
    Reject,
    Help,
    DispatchThenLoadConfiguration,
    LoadConfiguration,
}

fn startup_route(command: Option<ServiceCommand>) -> StartupRoute {
    match command {
        None => StartupRoute::Reject,
        Some(ServiceCommand::Help) => StartupRoute::Help,
        Some(ServiceCommand::Run) => StartupRoute::DispatchThenLoadConfiguration,
        Some(
            ServiceCommand::Install
            | ServiceCommand::Start
            | ServiceCommand::Stop
            | ServiceCommand::Remove,
        ) => StartupRoute::LoadConfiguration,
    }
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

fn run_service(config: ServiceConfig) -> Result<(), RuntimeWiringError> {
    config
        .validate()
        .map_err(|error| RuntimeWiringError::Configuration(error.to_string()))?;
    let session_id =
        process_session_id(&config.service_name).map_err(RuntimeWiringError::WorkerConstruction)?;
    let worker = RawTelemetryWorker::new(
        NativeMemoryTelemetry,
        AtomicRawTelemetryPublisher::new(&config.demand_report_path),
        SystemTelemetryClock::default(),
        config.vm_name,
        config.service_name,
        session_id,
        config.poll_interval,
    )
    .map_err(RuntimeWiringError::WorkerConstruction)?;

    let mut host = ServiceHost::with_shutdown_timeout(worker, config.shutdown_timeout);
    host.run()
        .map_err(|error| RuntimeWiringError::HostExecution(error.to_string()))?;

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = parse_command(&args);
    let route = startup_route(command);

    match route {
        StartupRoute::Reject => {
            eprintln!("unknown command; use 'help' for usage");
            process::exit(2);
        }
        StartupRoute::Help => {
            println!("Usage: virtio-mem-service [install|start|run|stop|remove|help]");
            return;
        }
        StartupRoute::DispatchThenLoadConfiguration => {}
        StartupRoute::LoadConfiguration => {}
    }
    if route == StartupRoute::DispatchThenLoadConfiguration {
        match run_as_service() {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                eprintln!("service dispatcher failed: {error}");
                process::exit(1);
            }
        }
    }
    let config = match ServiceConfig::load_default() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("service configuration failed: {error}");
            process::exit(1);
        }
    };
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
        Some(ServiceCommand::Run) => {
            if let Err(error) = run_service(config) {
                eprintln!("virtio-mem service runtime failed: {error}");
                process::exit(1);
            }
        }
        Some(ServiceCommand::Help) | None => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ServiceConfig {
        ServiceConfig {
            vm_name: "test-vm".to_owned(),
            service_name: "TestService".to_owned(),
            display_name: "Test service".to_owned(),
            description: "Test configuration".to_owned(),
            qga_pipe_path: r"\\.\pipe\test-qga".to_owned(),
            demand_report_path: r"C:\test\telemetry.jsonl".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            config_path: r"C:\test\config.json".to_owned(),
            poll_interval: std::time::Duration::from_millis(20),
            qga_operation_timeout: std::time::Duration::from_millis(10),
            shutdown_timeout: std::time::Duration::from_millis(30),
        }
    }

    #[test]
    fn rejects_invalid_service_configuration_before_startup() {
        let config = ServiceConfig {
            service_name: String::new(),
            ..test_config()
        };

        assert_eq!(
            run_service(config),
            Err(RuntimeWiringError::Configuration(
                "service configuration field is empty: service name".to_owned()
            ))
        );
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

    #[test]
    fn service_run_dispatches_before_configuration_loading() {
        assert_eq!(
            startup_route(Some(ServiceCommand::Run)),
            StartupRoute::DispatchThenLoadConfiguration
        );
        assert_eq!(
            startup_route(Some(ServiceCommand::Install)),
            StartupRoute::LoadConfiguration
        );
    }
}
