use std::io;
use std::process::{Command, Stdio};
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
}

pub trait VirshCommand {
    fn run(&self, arguments: &[String]) -> Result<String, VirshError>;
}

#[derive(Debug, Clone)]
pub struct Virsh {
    binary: String,
    timeout: Duration,
    connection: Option<String>,
}

impl Virsh {
    pub fn new(binary: impl Into<String>, timeout: Duration) -> Self {
        Self {
            binary: binary.into(),
            timeout,
            connection: None,
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
            connection: Some(connection.into()),
        }
    }
}

impl VirshCommand for Virsh {
    fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
        let mut command = Command::new(&self.binary);
        if let Some(connection) = &self.connection {
            command.args(["-c", connection]);
        }
        let mut child = command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if child.wait_timeout(self.timeout)?.is_none() {
            child.kill()?;
            let _ = child.wait();
            return Err(VirshError::Timeout(self.timeout));
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(VirshError::Failed {
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
