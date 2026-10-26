use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use thiserror::Error;
use wait_timeout::ChildExt;

#[derive(Debug, Error)]
pub enum VirshError {
    #[error("failed to start virsh: {0}")]
    Start(#[from] io::Error),
    #[error("virsh timed out after {0:?}")]
    Timeout(Duration),
    #[error("virsh failed with status {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("virsh returned invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    // Polkit authorizes root for every action and libvirt accepts root on the
    // local socket, so sudo is a valid manual test path; it is not a deployment.
    #[error("non-interactive libvirt management access is unavailable: {0}; configure authorization for the host service account before starting the controller, or run it with sudo for manual testing")]
    Authorization(String),
}

pub trait VirshCommand {
    fn run(&self, arguments: &[String]) -> Result<String, VirshError>;
}

#[derive(Debug, Clone)]
pub struct Virsh {
    binary: String,
    timeout: Duration,
    connection: String,
}

impl Virsh {
    pub fn new(binary: impl Into<String>, timeout: Duration) -> Self {
        Self {
            binary: binary.into(),
            timeout,
            // An absent or non-Unicode optional environment value selects the
            // documented local host connection, rather than driver autodetection.
            connection: std::env::var("LIBVIRT_DEFAULT_URI")
                .unwrap_or_else(|_| "qemu:///system".to_owned()),
        }
    }

    pub fn with_connection(
        binary: impl Into<String>,
        timeout: Duration,
        connection: impl Into<String>,
    ) -> Self {
        Self {
            binary: binary.into(),
            timeout,
            connection: connection.into(),
        }
    }

    /// Check the local RHEL polkit policy without permitting a password dialog.
    /// This is a prerequisite check, not a grant or a substitute for libvirt's
    /// own checks. Remote transports retain their own authentication contract.
    pub fn require_management_access(&self) -> Result<(), VirshError> {
        if self.connection != "qemu:///system" {
            return Ok(());
        }
        let mut check = authorization_command().map_err(|error| {
            VirshError::Authorization(format!("identify controller process: {error}"))
        })?;
        let output = bounded_output(&mut check, self.timeout).map_err(|error| {
            VirshError::Authorization(format!("check polkit authorization: {error}"))
        })?;
        if !output.status.success() {
            return Err(VirshError::Authorization(format!(
                "pkcheck {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn command(&self, arguments: &[String]) -> Command {
        let mut command = Command::new(&self.binary);
        command.args(["-c", &self.connection]);
        if read_only(arguments) {
            command.arg("--readonly");
        }
        command.args(arguments);
        command
    }

    fn run_with_access_check(
        &self,
        arguments: &[String],
        check: impl FnOnce() -> Result<(), VirshError>,
    ) -> Result<String, VirshError> {
        if !read_only(arguments) {
            check()?;
        }
        let output =
            bounded_output(&mut self.command(arguments), self.timeout).map_err(|error| {
                if error.kind() == io::ErrorKind::TimedOut {
                    VirshError::Timeout(self.timeout)
                } else {
                    VirshError::Start(error)
                }
            })?;
        if !output.status.success() {
            return Err(VirshError::Failed {
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

impl VirshCommand for Virsh {
    fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
        self.run_with_access_check(arguments, || self.require_management_access())
    }
}

fn read_only(arguments: &[String]) -> bool {
    matches!(
        arguments.first().map(String::as_str),
        Some("dumpxml" | "dommemstat" | "domstate" | "version")
    )
}

fn authorization_command() -> io::Result<Command> {
    let identity = process_identity()?;
    let mut command = Command::new("pkcheck");
    // Never add --allow-user-interaction or --enable-internal-agent: this check
    // must not ask the desktop user to authenticate each connection.
    command.args([
        "--action-id",
        "org.libvirt.unix.manage",
        "--process",
        &identity,
    ]);
    Ok(command)
}

#[cfg(target_os = "linux")]
fn process_identity() -> io::Result<String> {
    use std::os::unix::fs::MetadataExt;

    let stat = std::fs::read_to_string("/proc/self/stat")?;
    // comm (field 2) may contain spaces and parentheses. starttime is field 22.
    let started = stat
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .ok_or_else(|| io::Error::other("/proc/self/stat has no start time"))?
        .parse::<u64>()
        .map_err(io::Error::other)?;
    let uid = std::fs::metadata("/proc/self")?.uid();
    Ok(format!("{},{started},{uid}", std::process::id()))
}

#[cfg(not(target_os = "linux"))]
fn process_identity() -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "local system libvirt authorization requires the Linux host",
    ))
}

fn bounded_output(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout is unavailable"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr is unavailable"))?;
    std::thread::scope(|scope| {
        let stdout_reader = scope.spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).map(|_| bytes)
        });
        let stderr_reader = scope.spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        });
        let waited = child.wait_timeout(timeout);
        if !matches!(&waited, Ok(Some(_))) {
            let terminate = child.kill();
            let reap = child.wait();
            terminate?;
            reap?;
        }
        let status =
            waited?.ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "command timed out"));
        let stdout = stdout_reader
            .join()
            .map_err(|_| io::Error::other("stdout reader panicked"))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| io::Error::other("stderr reader panicked"))??;
        Ok(Output {
            status: status?,
            stdout,
            stderr,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args(["--exact", "virsh::tests::process_fixture", "--nocapture"])
            .env("VIRTIO_MEM_VIRSH_FIXTURE", mode);
        command
    }

    #[test]
    fn process_fixture() {
        use std::io::Write;

        let Ok(mode) = std::env::var("VIRTIO_MEM_VIRSH_FIXTURE") else {
            return;
        };
        match mode.as_str() {
            "output" => {
                let bytes = vec![b'x'; 256 * 1024];
                io::stdout().write_all(&bytes).expect("fixture stdout");
                io::stderr().write_all(&bytes).expect("fixture stderr");
            }
            "failure" => {
                eprintln!("fixture access denied");
                std::process::exit(7);
            }
            "timeout" => std::thread::sleep(Duration::from_secs(10)),
            other => panic!("unexpected fixture mode: {other}"),
        }
    }

    #[test]
    fn drains_large_stdout_and_stderr_before_waiting_for_exit() {
        let output = bounded_output(&mut fixture("output"), Duration::from_secs(5))
            .expect("large output does not block the child");
        assert!(output.status.success());
        assert!(output.stdout.len() >= 256 * 1024);
        assert_eq!(output.stderr.len(), 256 * 1024);
    }

    #[test]
    fn preserves_failed_command_status_and_stderr() {
        let output = bounded_output(&mut fixture("failure"), Duration::from_secs(5))
            .expect("capture failed child");
        assert_eq!(output.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&output.stderr).contains("fixture access denied"));
    }

    #[test]
    fn terminates_a_command_that_exceeds_its_deadline() {
        let error = bounded_output(&mut fixture("timeout"), Duration::from_millis(100))
            .expect_err("sleeping child must time out");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn read_commands_do_not_request_a_management_connection() {
        let virsh = Virsh::with_connection("virsh", Duration::from_secs(1), "qemu:///system");
        for operation in ["dumpxml", "dommemstat", "domstate", "version"] {
            let command = virsh.command(&[operation.to_owned(), "guest".to_owned()]);
            let args = command.get_args().collect::<Vec<_>>();
            assert_eq!(
                args,
                ["-c", "qemu:///system", "--readonly", operation, "guest"]
            );
        }
        let command = virsh.command(&["update-memory-device".to_owned(), "guest".to_owned()]);
        assert!(!command.get_args().any(|arg| arg == "--readonly"));
    }

    #[test]
    fn denied_authorization_prevents_starting_a_management_command() {
        let virsh = Virsh::with_connection(
            "missing-virsh-fixture",
            Duration::from_secs(1),
            "qemu:///system",
        );
        for operation in [
            "update-memory-device",
            "qemu-agent-command",
            "qemu-monitor-command",
        ] {
            let error = virsh
                .run_with_access_check(&[operation.to_owned()], || {
                    Err(VirshError::Authorization(
                        "service access denied".to_owned(),
                    ))
                })
                .expect_err("authorization must precede command execution");
            assert!(matches!(error, VirshError::Authorization(_)));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn authorization_check_is_non_interactive_and_identifies_the_exact_process() {
        use std::os::unix::fs::MetadataExt;

        let command = authorization_command().expect("construct local policy check");
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(command.get_program(), "pkcheck");
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], "--action-id");
        assert_eq!(args[1], "org.libvirt.unix.manage");
        assert_eq!(args[2], "--process");
        let identity = args[3].to_str().expect("process identity");
        let fields = identity.split(',').collect::<Vec<_>>();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], std::process::id().to_string());
        assert!(fields[1].parse::<u64>().expect("process start ticks") > 0);
        assert_eq!(
            fields[2],
            std::fs::metadata("/proc/self")
                .expect("metadata")
                .uid()
                .to_string()
        );
    }

    struct RecordingVirsh {
        arguments: Vec<String>,
    }
    impl VirshCommand for RecordingVirsh {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            assert_eq!(arguments, self.arguments);
            Ok("ok".to_owned())
        }
    }
    #[test]
    fn trait_accepts_exact_argument_vectors() {
        assert_eq!(
            RecordingVirsh {
                arguments: vec![
                    "dumpxml".to_owned(),
                    "--live".to_owned(),
                    "guest".to_owned()
                ]
            }
            .run(&[
                "dumpxml".to_owned(),
                "--live".to_owned(),
                "guest".to_owned()
            ])
            .expect("recorded command"),
            "ok"
        );
    }
}
