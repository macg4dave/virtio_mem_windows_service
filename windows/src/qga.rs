use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};

use crate::runtime::GuestAgent;

const GET_MEMORY_STATS_REQUEST: &str = r#"{"execute":"guest-get-memory-stats"}"#;

pub struct NamedPipeGuestAgent {
    pipe_path: String,
}

impl NamedPipeGuestAgent {
    pub fn new(pipe_path: impl Into<String>) -> Self {
        Self {
            pipe_path: pipe_path.into(),
        }
    }
}

impl GuestAgent for NamedPipeGuestAgent {
    fn get_memory_stats(&mut self) -> Result<String, String> {
        let mut pipe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.pipe_path)
            .map_err(|error| format!("open {}: {error}", self.pipe_path))?;

        writeln!(pipe, "{GET_MEMORY_STATS_REQUEST}")
            .map_err(|error| format!("write request: {error}"))?;
        pipe.flush()
            .map_err(|error| format!("flush request: {error}"))?;

        let mut response = String::new();
        BufReader::new(pipe)
            .read_line(&mut response)
            .map_err(|error| format!("read response: {error}"))?;

        if response.trim().is_empty() {
            return Err("QEMU Guest Agent returned an empty response".to_owned());
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_configured_pipe_path() {
        let agent = NamedPipeGuestAgent::new(r"\\.\pipe\qga-test");

        assert_eq!(agent.pipe_path, r"\\.\pipe\qga-test");
    }
}
