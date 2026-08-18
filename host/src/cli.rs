use std::time::Duration;

use virtio_mem_core::parse_virtio_mem_xml_for_alias;

use crate::resize_sink::VirshResizeSink;
use crate::virsh::{Virsh, VirshCommand};

const DEFAULT_CONNECTION: &str = "qemu:///system";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Snapshot {
        vm: String,
        alias: String,
        connection: String,
    },
    Validate {
        vm: String,
        alias: String,
        connection: String,
    },
    Resize {
        vm: String,
        alias: String,
        target_bytes: u64,
        connection: String,
        apply: bool,
    },
}

pub fn parse_args(args: &[String]) -> Result<Option<CliCommand>, String> {
    let Some(mode) = args.first().map(String::as_str) else {
        return Ok(None);
    };
    if !matches!(mode, "snapshot" | "validate" | "resize") {
        return Ok(None);
    }
    let (minimum, maximum) = if mode == "resize" { (4, 6) } else { (3, 5) };
    if args.len() < minimum || args.len() > maximum {
        return Err(usage().to_owned());
    }
    let vm = args[1].clone();
    let alias = args[2].clone();
    let mut connection = DEFAULT_CONNECTION.to_owned();
    let mut apply = false;
    let mut target_bytes = None;
    let mut index = 3;
    if mode == "resize" {
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
            "--apply" if mode == "resize" => apply = true,
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
    Ok(Some(match mode {
        "snapshot" => CliCommand::Snapshot {
            vm,
            alias,
            connection,
        },
        "validate" => CliCommand::Validate {
            vm,
            alias,
            connection,
        },
        "resize" => CliCommand::Resize {
            vm,
            alias,
            target_bytes: target_bytes.expect("resize target is parsed above"),
            connection,
            apply,
        },
        _ => unreachable!(),
    }))
}

pub fn usage() -> &'static str {
    "Usage: virtio-mem-host [snapshot|validate] VM ALIAS [--connect URI]\n       virtio-mem-host resize VM ALIAS TARGET_BYTES [--apply] [--connect URI]"
}

pub fn run(command: CliCommand) -> Result<(), String> {
    match command {
        CliCommand::Snapshot {
            vm,
            alias,
            connection,
        } => {
            let virsh = Virsh::with_connection("virsh", DEFAULT_TIMEOUT, connection);
            let xml = virsh
                .run(&["dumpxml".to_owned(), vm])
                .map_err(|error| error.to_string())?;
            parse_virtio_mem_xml_for_alias(&xml, &alias)
                .map_err(|error| error.to_string())?;
            print!("{xml}");
            Ok(())
        }
        CliCommand::Validate {
            vm,
            alias,
            connection,
        } => {
            let virsh = Virsh::with_connection("virsh", DEFAULT_TIMEOUT, connection);
            let xml = virsh
                .run(&["dumpxml".to_owned(), vm])
                .map_err(|error| error.to_string())?;
            let snapshot = parse_virtio_mem_xml_for_alias(&xml, &alias)
                .map_err(|error| error.to_string())?;
            println!("alias={}", snapshot.alias);
            println!("size_bytes={}", snapshot.memory.size_bytes);
            println!("block_size_bytes={}", snapshot.memory.block_size_bytes);
            println!("requested_bytes={}", snapshot.memory.requested_bytes);
            println!("current_bytes={}", snapshot.memory.current_bytes);
            println!("compatibility={:?}", snapshot.compatibility);
            Ok(())
        }
        CliCommand::Resize {
            vm,
            alias,
            target_bytes,
            connection,
            apply,
        } => {
            let virsh = Virsh::with_connection("virsh", DEFAULT_TIMEOUT, connection);
            let sink = VirshResizeSink::new(virsh, vm, alias);
            let arguments = sink.prepare_resize(target_bytes)?;
            if apply {
                sink.request_resize(target_bytes)?;
                println!("resize applied");
            } else {
                println!("dry-run: virsh {}", arguments.join(" "));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_snapshot_and_resize_modes() {
        assert_eq!(
            parse_args(&args(&["snapshot", "guest", "memory0"])).expect("snapshot"),
            Some(CliCommand::Snapshot {
                vm: "guest".to_owned(),
                alias: "memory0".to_owned(),
                connection: DEFAULT_CONNECTION.to_owned(),
            })
        );
        assert!(matches!(
            parse_args(&args(&["resize", "guest", "memory0", "2097152", "--apply"]))
                .expect("resize"),
            Some(CliCommand::Resize { apply: true, .. })
        ));
    }

    #[test]
    fn rejects_invalid_cli_options_and_targets() {
        assert!(parse_args(&args(&["resize", "guest", "memory0", "0"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--connect"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--unknown"])).is_err());
    }
}