use std::io::Write;
use std::time::Duration;

use virtio_mem_core::{parse_virtio_mem_xml_for_alias, CompatibilityEvidence};

use crate::compatibility_source::{CompatibilitySource, VirshQmpCompatibilitySource};
use crate::host_memory::{validate_grow_headroom, HostMemorySource, ProcMeminfoSource};
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
        workload_reviewed: bool,
        host_min_headroom_bytes: u64,
    },
}

pub fn parse_args(args: &[String]) -> Result<Option<CliCommand>, String> {
    let Some(mode) = args.first().map(String::as_str) else {
        return Ok(None);
    };
    if !matches!(mode, "snapshot" | "validate" | "resize") {
        return Err(format!("unknown CLI command: {mode}\n{}", usage()));
    }
    let (minimum, maximum) = if mode == "resize" { (7, 10) } else { (3, 5) };
    if args.len() < minimum || args.len() > maximum {
        return Err(usage().to_owned());
    }
    let vm = args[1].clone();
    let alias = args[2].clone();
    let mut connection = DEFAULT_CONNECTION.to_owned();
    let mut apply = false;
    let mut workload_reviewed = false;
    let mut host_min_headroom_bytes = None;
    let mut connection_supplied = false;
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
            "--apply" if mode == "resize" && !apply => apply = true,
            "--workload-reviewed" if mode == "resize" && !workload_reviewed => {
                workload_reviewed = true;
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
    if mode == "resize" && !workload_reviewed {
        return Err("resize requires --workload-reviewed".to_owned());
    }
    let command = match mode {
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
            target_bytes: target_bytes
                .ok_or_else(|| "resize target was not supplied".to_owned())?,
            connection,
            apply,
            workload_reviewed,
            host_min_headroom_bytes: host_min_headroom_bytes
                .ok_or_else(|| "resize requires --host-min-headroom-bytes BYTES".to_owned())?,
        },
        _ => return Err(format!("unknown CLI command: {mode}")),
    };
    Ok(Some(command))
}

pub fn usage() -> &'static str {
    "Usage: virtio-mem-host [snapshot|validate] VM ALIAS [--connect URI]\n       virtio-mem-host resize VM ALIAS TARGET_BYTES --workload-reviewed --host-min-headroom-bytes BYTES [--apply] [--connect URI]"
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
        CliCommand::Snapshot {
            vm,
            alias,
            connection,
        } => {
            let virsh = Virsh::with_connection("virsh", DEFAULT_TIMEOUT, connection);
            run_snapshot_with(virsh, &vm, &alias, output)
        }
        CliCommand::Validate {
            vm,
            alias,
            connection,
        } => {
            let virsh = Virsh::with_connection("virsh", DEFAULT_TIMEOUT, connection);
            run_validate_with(virsh, &vm, &alias, output)
        }
        CliCommand::Resize {
            vm,
            alias,
            target_bytes,
            connection,
            apply,
            workload_reviewed,
            host_min_headroom_bytes,
        } => {
            let virsh = Virsh::with_connection("virsh", DEFAULT_TIMEOUT, &connection);
            let compatibility_source = VirshQmpCompatibilitySource::new(
                virsh.clone(),
                vm.clone(),
                alias.clone(),
                if workload_reviewed {
                    CompatibilityEvidence::Confirmed
                } else {
                    CompatibilityEvidence::Unknown
                },
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
    }
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::compatibility_source::FixedCompatibilitySource;
    use crate::virsh::VirshError;
    use virtio_mem_core::VirtioMemCompatibility;

    const GIB: u64 = 1 << 30;
    const CONVERGED_XML: &str = "<domain><devices><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></devices></domain>";

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
            parse_args(&args(&["snapshot", "guest", "memory0"])).expect("snapshot"),
            Some(CliCommand::Snapshot {
                vm: "guest".to_owned(),
                alias: "memory0".to_owned(),
                connection: DEFAULT_CONNECTION.to_owned(),
            })
        );
        assert!(matches!(
            parse_args(&args(&[
                "resize",
                "guest",
                "memory0",
                "2097152",
                "--workload-reviewed",
                "--host-min-headroom-bytes",
                "1073741824",
                "--apply"
            ]))
            .expect("resize"),
            Some(CliCommand::Resize { apply: true, .. })
        ));
    }

    #[test]
    fn rejects_invalid_cli_options_and_targets() {
        assert!(parse_args(&args(&["resize", "guest", "memory0", "0"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--connect"])).is_err());
        assert!(parse_args(&args(&["snapshot", "guest", "memory0", "--unknown"])).is_err());
        assert!(parse_args(&args(&["unknown"])).is_err());
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
