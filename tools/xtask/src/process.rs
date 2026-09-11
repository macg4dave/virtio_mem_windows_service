use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

pub fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub fn run(program: &str, args: &[&str], cwd: &Path) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|error| format!("failed to start {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed with status {status}"))
    }
}

pub fn output(program: &str, args: &[OsString], cwd: &Path) -> Result<Output, String> {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("failed to start {program}: {error}"))
}

pub fn checked_output(program: &str, args: &[OsString], cwd: &Path) -> Result<Vec<u8>, String> {
    let output = output(program, args, cwd)?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

pub fn checked_text(program: &str, args: &[OsString], cwd: &Path) -> Result<String, String> {
    String::from_utf8(checked_output(program, args, cwd)?)
        .map_err(|error| format!("{program} returned invalid UTF-8: {error}"))
}

pub fn bounded_text(
    program: &str,
    args: &[OsString],
    cwd: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let output = bounded_output(program, args, cwd, timeout)?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} returned invalid UTF-8: {error}"))
}

pub fn bounded_output(
    program: &str,
    args: &[OsString],
    cwd: &Path,
    timeout: Duration,
) -> Result<Output, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {program}: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("failed to capture {program} stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("failed to capture {program} stderr"))?;
    let stdout_reader = std::thread::spawn(move || read_all(stdout));
    let stderr_reader = std::thread::spawn(move || read_all(stderr));
    let status = if let Some(status) = child
        .wait_timeout(timeout)
        .map_err(|error| format!("failed while waiting for {program}: {error}"))?
    {
        status
    } else {
        child
            .kill()
            .map_err(|error| format!("failed to terminate timed-out {program}: {error}"))?;
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err(format!("{program} timed out after {timeout:?}"));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| format!("{program} stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| format!("{program} stderr reader panicked"))??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_all(mut input: impl Read) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    input
        .read_to_end(&mut output)
        .map_err(|error| format!("failed to read command output: {error}"))?;
    Ok(output)
}

pub fn run_with_input(
    program: &str,
    args: &[&str],
    cwd: &Path,
    input: &[u8],
) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {program}: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| format!("failed to open {program} stdin"))?
        .write_all(input)
        .map_err(|error| format!("failed to write {program} input: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("failed to wait for {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed with status {status}"))
    }
}

pub fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_os_string()
}

pub fn read_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let mut value = String::new();
    file.read_to_string(&mut value)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_preserves_plain_arguments() {
        assert_eq!(os("value"), OsString::from("value"));
    }
}
