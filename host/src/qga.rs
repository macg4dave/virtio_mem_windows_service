use serde::Deserialize;
use serde_json::{json, Value};
use virtio_mem_core::{parse_memory_stats, MemoryStats};

use crate::runtime::GuestStatsSource;
use crate::virsh::VirshCommand;

const GET_MEMORY_STATS_REQUEST: &str = r#"{"execute":"guest-get-memory-stats"}"#;
const MAX_GUEST_FILE_READS: usize = 16;

pub trait GuestFileReader {
    fn read_file(&self, path: &str, maximum_bytes: usize) -> Result<Vec<u8>, String>;
}

pub struct VirshQgaFileReader<C> {
    command: C,
    vm_name: String,
}

impl<C> VirshQgaFileReader<C> {
    pub fn new(command: C, vm_name: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
        }
    }
}

#[derive(Deserialize)]
struct OpenResponse {
    #[serde(rename = "return")]
    handle: u64,
}

#[derive(Deserialize)]
struct ReadResponse {
    #[serde(rename = "return")]
    result: ReadResult,
}

#[derive(Deserialize)]
struct ReadResult {
    count: usize,
    #[serde(rename = "buf-b64", default)]
    buffer_base64: String,
    eof: bool,
}

impl<C: VirshCommand> VirshQgaFileReader<C> {
    fn execute(&self, request: Value) -> Result<String, String> {
        self.command
            .run(&[
                "qemu-agent-command".to_owned(),
                self.vm_name.clone(),
                request.to_string(),
            ])
            .map_err(|error| error.to_string())
    }

    fn close(&self, handle: u64) -> Result<(), String> {
        let response = self.execute(json!({
            "execute": "guest-file-close",
            "arguments": {"handle": handle}
        }))?;
        let response: Value = serde_json::from_str(&response)
            .map_err(|error| format!("parse guest-file-close response: {error}"))?;
        if response.get("return").and_then(Value::as_object).is_none() {
            return Err("guest-file-close response has no return object".to_owned());
        }
        Ok(())
    }
}

impl<C: VirshCommand> GuestFileReader for VirshQgaFileReader<C> {
    fn read_file(&self, path: &str, maximum_bytes: usize) -> Result<Vec<u8>, String> {
        if path.trim().is_empty() {
            return Err("QGA telemetry path must not be empty".to_owned());
        }
        let response = self.execute(json!({
            "execute": "guest-file-open",
            "arguments": {"path": path, "mode": "r"}
        }))?;
        let response: OpenResponse = serde_json::from_str(&response)
            .map_err(|error| format!("parse guest-file-open response: {error}"))?;
        let result = (|| {
            let mut contents = Vec::new();
            for _ in 0..MAX_GUEST_FILE_READS {
                let requested = maximum_bytes
                    .checked_sub(contents.len())
                    .and_then(|remaining| remaining.checked_add(1))
                    .ok_or_else(|| "QGA telemetry read bound overflowed".to_owned())?;
                let read = self.execute(json!({
                    "execute": "guest-file-read",
                    "arguments": {"handle": response.handle, "count": requested}
                }))?;
                let read: ReadResponse = serde_json::from_str(&read)
                    .map_err(|error| format!("parse guest-file-read response: {error}"))?;
                let bytes = decode_base64(&read.result.buffer_base64)?;
                if read.result.count != bytes.len() {
                    return Err(format!(
                        "guest-file-read count {} does not match decoded length {}",
                        read.result.count,
                        bytes.len()
                    ));
                }
                if bytes.is_empty() && !read.result.eof {
                    return Err("QGA guest-file-read made no progress before EOF".to_owned());
                }
                contents.extend_from_slice(&bytes);
                if contents.len() > maximum_bytes {
                    return Err(format!(
                        "QGA telemetry file exceeds {maximum_bytes} byte limit"
                    ));
                }
                if read.result.eof {
                    return Ok(contents);
                }
            }
            Err(format!(
                "QGA telemetry file did not reach EOF within {MAX_GUEST_FILE_READS} reads"
            ))
        })();
        let close = self.close(response.handle);
        match (result, close) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(close_error)) => Err(close_error),
            (Err(error), Err(close_error)) => Err(format!(
                "{error}; additionally failed to close QGA handle: {close_error}"
            )),
        }
    }
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(4) {
        return Err("QGA base64 payload length is invalid".to_owned());
    }
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    for (index, chunk) in input.as_bytes().chunks_exact(4).enumerate() {
        let last = index + 1 == input.len() / 4;
        let padding = match (chunk[2], chunk[3]) {
            (b'=', b'=') => 2,
            (_, b'=') => 1,
            (_, _) => 0,
        };
        if padding > 0 && !last {
            return Err("QGA base64 padding appears before the final block".to_owned());
        }
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3])?
        };
        if chunk[0] == b'=' || chunk[1] == b'=' || (chunk[2] == b'=' && chunk[3] != b'=') {
            return Err("QGA base64 padding is invalid".to_owned());
        }
        output.push((a << 2) | (b >> 4));
        if padding < 2 {
            output.push((b << 4) | (c >> 2));
        }
        if padding == 0 {
            output.push((c << 6) | d);
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("QGA base64 payload contains an invalid character".to_owned()),
    }
}

/// Memory-stat source backed by the QEMU Guest Agent `guest-get-memory-stats`
/// command. Requires a guest agent that implements that command; see
/// [`crate::dommemstat`] for a fallback that does not depend on it.
pub struct VirshGuestAgent<C> {
    command: C,
    vm_name: String,
}

impl<C> VirshGuestAgent<C> {
    pub fn new(command: C, vm_name: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
        }
    }
}

impl<C: VirshCommand> GuestStatsSource for VirshGuestAgent<C> {
    fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        let response = self
            .command
            .run(&[
                "qemu-agent-command".to_owned(),
                self.vm_name.clone(),
                GET_MEMORY_STATS_REQUEST.to_owned(),
            ])
            .map_err(|error| error.to_string())?;
        parse_memory_stats(&response).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virsh::VirshError;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    struct Fake;
    impl VirshCommand for Fake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            assert_eq!(
                arguments,
                &[
                    "qemu-agent-command".to_owned(),
                    "guest".to_owned(),
                    GET_MEMORY_STATS_REQUEST.to_owned()
                ]
            );
            Ok(r#"{"return":[{"stat":"stat-free","value":100},{"stat":"stat-total","value":200}]}"#.to_owned())
        }
    }
    #[test]
    fn uses_qga_memory_stats_command() {
        let stats = VirshGuestAgent::new(Fake, "guest")
            .get_memory_stats()
            .expect("command succeeds");
        assert_eq!(stats.total_bytes, 200);
    }

    struct FileFake {
        responses: Mutex<VecDeque<String>>,
        requests: Mutex<Vec<Value>>,
    }

    impl FileFake {
        fn new(responses: &[&str]) -> Self {
            Self {
                responses: Mutex::new(responses.iter().map(|value| (*value).to_owned()).collect()),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl VirshCommand for FileFake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            assert_eq!(&arguments[..2], &["qemu-agent-command", "guest"]);
            self.requests
                .lock()
                .expect("requests")
                .push(serde_json::from_str(&arguments[2]).expect("request JSON"));
            Ok(self
                .responses
                .lock()
                .expect("responses")
                .pop_front()
                .expect("response"))
        }
    }

    #[test]
    fn reads_and_closes_one_bounded_guest_file() {
        let fake = FileFake::new(&[
            r#"{"return":7}"#,
            r#"{"return":{"count":3,"buf-b64":"YWJj","eof":true}}"#,
            r#"{"return":{}}"#,
        ]);
        assert_eq!(
            VirshQgaFileReader::new(fake, "guest").read_file(r"C:\telemetry.json", 3),
            Ok(b"abc".to_vec())
        );
    }

    #[test]
    fn joins_bounded_partial_qga_reads_before_closing() {
        let fake = FileFake::new(&[
            r#"{"return":7}"#,
            r#"{"return":{"count":2,"buf-b64":"YWI=","eof":false}}"#,
            r#"{"return":{"count":1,"buf-b64":"Yw==","eof":true}}"#,
            r#"{"return":{}}"#,
        ]);
        assert_eq!(
            VirshQgaFileReader::new(fake, "guest").read_file(r"C:\telemetry.json", 3),
            Ok(b"abc".to_vec())
        );
    }

    #[test]
    fn rejects_oversized_or_malformed_base64_and_still_closes() {
        for read_response in [
            r#"{"return":{"count":4,"buf-b64":"YWJjZA==","eof":true}}"#,
            r#"{"return":{"count":1,"buf-b64":"!===","eof":true}}"#,
        ] {
            let fake = FileFake::new(&[r#"{"return":9}"#, read_response, r#"{"return":{}}"#]);
            let reader = VirshQgaFileReader::new(fake, "guest");
            assert!(reader.read_file(r"C:\telemetry.json", 3).is_err());
            assert_eq!(reader.command.requests.lock().expect("requests").len(), 3);
        }
    }

    #[test]
    fn decodes_standard_base64_strictly() {
        assert_eq!(decode_base64(""), Ok(Vec::new()));
        assert_eq!(decode_base64("YQ=="), Ok(b"a".to_vec()));
        assert_eq!(decode_base64("YWI="), Ok(b"ab".to_vec()));
        assert_eq!(decode_base64("YWJj"), Ok(b"abc".to_vec()));
        assert!(decode_base64("YQ=").is_err());
    }
}
