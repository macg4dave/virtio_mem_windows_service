use serde::Deserialize;
use serde_json::{json, Value};

use crate::virsh::VirshCommand;

const GUEST_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub trait GuestFileReader {
    fn read_file(&self, path: &str, maximum_bytes: usize) -> Result<Vec<u8>, String>;
}

/// Replaces a guest file's contents. QGA cannot rename, so readers must
/// reject the truncated or partial contents they may observe mid-write.
pub trait GuestFileWriter {
    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), String>;
}

pub struct VirshGuestFiles<C> {
    command: C,
    vm_name: String,
}

impl<C> VirshGuestFiles<C> {
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
struct WriteResponse {
    #[serde(rename = "return")]
    result: WriteResult,
}

#[derive(Deserialize)]
struct WriteResult {
    count: usize,
}

impl<C: VirshCommand> VirshGuestFiles<C> {
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
        self.close_once(handle).or_else(|first| {
            // A timeout can leave a known QGA handle holding telemetry open.
            // Retry only closing that handle, never its read or write. The
            // second close is harmless if the first reply alone was lost.
            self.close_once(handle)
                .map_err(|second| format!("{first}; close retry failed: {second}"))
        })
    }

    fn close_once(&self, handle: u64) -> Result<(), String> {
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

    fn open(&self, path: &str, mode: &str) -> Result<u64, String> {
        if path.trim().is_empty() {
            return Err("QGA guest file path must not be empty".to_owned());
        }
        let response = self.execute(json!({
            "execute": "guest-file-open",
            "arguments": {"path": path, "mode": mode}
        }))?;
        serde_json::from_str::<OpenResponse>(&response)
            .map(|response| response.handle)
            .map_err(|error| format!("parse guest-file-open response: {error}"))
    }

    /// Runs `operation` on an open handle and always closes it, keeping the
    /// operation's error as the primary failure.
    fn with_handle<T>(
        &self,
        handle: u64,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let result = operation();
        let close = self.close(handle);
        match (result, close) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(close_error)) => Err(close_error),
            (Err(error), Err(close_error)) => Err(format!(
                "{error}; additionally failed to close QGA handle: {close_error}"
            )),
        }
    }
}

impl<C: VirshCommand> GuestFileWriter for VirshGuestFiles<C> {
    fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), String> {
        let handle = self.open(path, "w")?;
        self.with_handle(handle, || {
            let response = self.execute(json!({
                "execute": "guest-file-write",
                "arguments": {"handle": handle, "buf-b64": encode_base64(contents)}
            }))?;
            let response: WriteResponse = serde_json::from_str(&response)
                .map_err(|error| format!("parse guest-file-write response: {error}"))?;
            if response.result.count != contents.len() {
                return Err(format!(
                    "guest-file-write wrote {} of {} bytes",
                    response.result.count,
                    contents.len()
                ));
            }
            Ok(())
        })
    }
}

/// Read Windows handoffs without leaving a daemon-owned QGA file handle.
/// The child owns and closes its read handle, including when a host command
/// times out. Delete sharing lets the publisher atomically replace the file.
fn shared_reader_script(path: &str, maximum_bytes: usize) -> Result<String, String> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err("QGA guest file path must be nonempty and contain no NUL".to_owned());
    }
    let capacity = maximum_bytes
        .checked_add(1)
        .filter(|capacity| *capacity <= i32::MAX as usize)
        .ok_or_else(|| "QGA read bound is too large".to_owned())?;
    let path = path.replace('\'', "''");
    Ok(format!(
        r#"$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue';
$f=[IO.File]::Open('{path}',[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete));
try {{
  $b=New-Object byte[] {capacity}; $n=0;
  while ($n -lt $b.Length) {{ $c=$f.Read($b,$n,$b.Length-$n); if ($c -eq 0) {{ break }}; $n+=$c }};
  if ($n -gt {maximum_bytes}) {{ throw 'guest file exceeds {maximum_bytes} byte limit' }};
  [Console]::OpenStandardOutput().Write($b,0,$n)
}} finally {{ $f.Dispose() }}"#
    ))
}

impl<C: VirshCommand> GuestFileReader for VirshGuestFiles<C> {
    fn read_file(&self, path: &str, maximum_bytes: usize) -> Result<Vec<u8>, String> {
        let script = shared_reader_script(path, maximum_bytes)?;
        let encoded: Vec<_> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let response: Value = serde_json::from_str(&self.execute(json!({
            "execute": "guest-exec",
            "arguments": {
                "path": "powershell.exe",
                "arg": ["-NoProfile", "-NonInteractive", "-EncodedCommand", encode_base64(&encoded)],
                "capture-output": true
            }
        }))?).map_err(|error| format!("parse guest-exec response: {error}"))?;
        let pid = response
            .get("return")
            .and_then(|value| value.get("pid"))
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("guest-exec did not return a reader PID: {response}"))?;
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() >= GUEST_READ_TIMEOUT {
                return Err("guest telemetry reader did not exit within 5 seconds".to_owned());
            }
            let response: Value = serde_json::from_str(&self.execute(json!({
                "execute": "guest-exec-status", "arguments": {"pid": pid}
            }))?)
            .map_err(|error| format!("parse guest-exec-status response: {error}"))?;
            let status = response
                .get("return")
                .ok_or_else(|| format!("guest-exec-status failed: {response}"))?;
            match status.get("exited").and_then(Value::as_bool) {
                Some(false) => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    continue;
                }
                Some(true) => {}
                None => return Err("guest-exec-status has no exited flag".to_owned()),
            }
            if status.get("exitcode").and_then(Value::as_i64) != Some(0)
                || status.get("signal").is_some()
            {
                let error =
                    decode_base64(status.get("err-data").and_then(Value::as_str).unwrap_or(""))?;
                return Err(format!(
                    "guest telemetry reader failed: {}",
                    String::from_utf8_lossy(&error)
                ));
            }
            if status.get("out-truncated").and_then(Value::as_bool) == Some(true) {
                return Err("guest telemetry reader output was truncated".to_owned());
            }
            let bytes =
                decode_base64(status.get("out-data").and_then(Value::as_str).unwrap_or(""))?;
            if bytes.len() > maximum_bytes {
                return Err(format!("QGA guest file exceeds {maximum_bytes} byte limit"));
            }
            return Ok(bytes);
        }
    }
}

/// Standard padded base64, as QGA and PowerShell payloads require.
pub fn encode_base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let value = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        for index in 0..4 {
            if index <= chunk.len() {
                let sextet = (value >> (18 - 6 * index)) & 0x3f;
                output.push(char::from(ALPHABET[sextet as usize]));
            } else {
                output.push('=');
            }
        }
    }
    output
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virsh::VirshError;
    use std::collections::VecDeque;
    use std::sync::Mutex;
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
    fn reads_with_a_child_owned_delete_shared_handle_and_reaps_the_child() {
        let fake = FileFake::new(&[
            r#"{"return":{"pid":7}}"#,
            r#"{"return":{"exited":false}}"#,
            r#"{"return":{"exited":true,"exitcode":0,"out-data":"YWJj"}}"#,
        ]);
        let files = VirshGuestFiles::new(fake, "guest");
        assert_eq!(
            files
                .read_file("C:\\Dave's folder\\telemetry.json", 3)
                .unwrap(),
            b"abc"
        );
        let requests = files.command.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0]["execute"], "guest-exec");
        assert_eq!(requests[1]["execute"], "guest-exec-status");
        assert_eq!(requests[1]["arguments"]["pid"], 7);
        let bytes = decode_base64(requests[0]["arguments"]["arg"][3].as_str().unwrap()).unwrap();
        let script = String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(script.contains("Dave''s folder"));
        assert!(script.contains("[IO.FileShare]::Delete"));
        assert!(script.contains("finally { $f.Dispose() }"));
    }

    #[test]
    fn rejects_failed_truncated_oversized_or_malformed_child_output() {
        for status in [
            r#"{"return":{"exited":true,"exitcode":1,"err-data":"ZmFpbGVk"}}"#,
            r#"{"return":{"exited":true,"exitcode":0,"out-truncated":true}}"#,
            r#"{"return":{"exited":true,"exitcode":0,"out-data":"YWJjZA=="}}"#,
            r#"{"return":{"exited":true,"exitcode":0,"out-data":"!="}}"#,
            r#"{"error":{"desc":"guest command timed out"}}"#,
        ] {
            let fake = FileFake::new(&[r#"{"return":{"pid":7}}"#, status]);
            let files = VirshGuestFiles::new(fake, "guest");
            assert!(files.read_file("publication", 3).is_err());
            assert!(files
                .command
                .requests
                .lock()
                .unwrap()
                .iter()
                .all(|request| request["execute"] != "guest-file-open"));
        }
        assert!(shared_reader_script("", 3).is_err());
        assert!(shared_reader_script("publication", usize::MAX).is_err());
    }

    #[test]
    fn retries_a_failed_close_without_replaying_the_file_operation() {
        let fake = FileFake::new(&[
            r#"{"return":7}"#,
            r#"{"return":{"count":3}}"#,
            r#"{"error":{"desc":"guest agent timed out"}}"#,
            r#"{"return":{}}"#,
        ]);
        let files = VirshGuestFiles::new(fake, "guest");
        files.write_file("status", b"abc").unwrap();
        let requests = files.command.requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[2], requests[3]);
    }

    #[test]
    fn writes_the_complete_contents_and_closes() {
        let fake = FileFake::new(&[
            r#"{"return":7}"#,
            r#"{"return":{"count":3,"eof":false}}"#,
            r#"{"return":{}}"#,
        ]);
        let files = VirshGuestFiles::new(fake, "guest");
        files.write_file(r"C:\status.json", b"abc").unwrap();
        let requests = files.command.requests.lock().expect("requests");
        assert_eq!(requests[0]["arguments"]["mode"], "w");
        assert_eq!(requests[1]["arguments"]["buf-b64"], "YWJj");
        assert_eq!(requests[2]["execute"], "guest-file-close");

        let short = FileFake::new(&[
            r#"{"return":7}"#,
            r#"{"return":{"count":2,"eof":false}}"#,
            r#"{"return":{}}"#,
        ]);
        assert!(VirshGuestFiles::new(short, "guest")
            .write_file(r"C:\status.json", b"abc")
            .is_err());
    }

    #[test]
    fn base64_round_trips_every_padding_length() {
        for input in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            &[0xff, 0x00, 0x7f, 0x80],
        ] {
            assert_eq!(decode_base64(&encode_base64(input)), Ok(input.to_vec()));
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
