use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::process;

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub vm: String,
    pub attempts: u32,
    pub connect: String,
    pub command_timeout_seconds: u64,
}

pub fn parse(args: &[String]) -> Result<Options, String> {
    let vm = args
        .first()
        .filter(|value| valid_scope(value))
        .cloned()
        .ok_or_else(|| {
            "qga requires a non-empty VM name that does not begin with '-'".to_owned()
        })?;
    let mut attempts = None;
    let mut connect = "qemu:///system".to_owned();
    let mut command_timeout_seconds = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--attempts" => {
                let parsed = value(args, &mut index, "--attempts")?
                    .parse::<u32>()
                    .map_err(|_| "--attempts requires a positive integer".to_owned())?;
                if parsed == 0 {
                    return Err("--attempts requires a positive integer".to_owned());
                }
                attempts = Some(parsed);
            }
            "--connect" => connect = value(args, &mut index, "--connect")?,
            "--command-timeout-seconds" => {
                let parsed = value(args, &mut index, "--command-timeout-seconds")?
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        "--command-timeout-seconds requires a positive integer".to_owned()
                    })?;
                command_timeout_seconds = Some(parsed);
            }
            option => return Err(format!("unknown qga option: {option}")),
        }
        index += 1;
    }
    if !valid_scope(&connect) {
        return Err("--connect must be non-empty and contain no control characters".to_owned());
    }
    Ok(Options {
        vm,
        attempts: attempts.ok_or_else(|| "--attempts is required".to_owned())?,
        connect,
        command_timeout_seconds: command_timeout_seconds
            .ok_or_else(|| "--command-timeout-seconds is required".to_owned())?,
    })
}

pub fn run(options: &Options, repo: &Path) -> Result<(), String> {
    if !process::command_exists("virsh") {
        return Err("missing required host command: virsh".to_owned());
    }
    println!("Checking QEMU Guest Agent for VM {}...", options.vm);
    let info = qga_command(options, repo, r#"{"execute":"guest-info"}"#)?;
    let info_json: Value = serde_json::from_str(&info)
        .map_err(|error| format!("guest-info returned invalid JSON: {error}"))?;
    if info_json.get("return").is_none() {
        return Err("guest-info response is missing return".to_owned());
    }
    println!("guest-info: OK");

    match qga_command(options, repo, r#"{"execute":"guest-get-memory-stats"}"#) {
        Ok(stats) => {
            validate_memory_stats(&stats)?;
            for attempt in 1..=options.attempts {
                let stats = qga_command(options, repo, r#"{"execute":"guest-get-memory-stats"}"#)?;
                validate_memory_stats(&stats)?;
                println!(
                    "guest-get-memory-stats attempt {attempt}/{}: OK",
                    options.attempts
                );
            }
        }
        Err(_) => {
            println!("guest-get-memory-stats is unavailable; validating dommemstat fallback.");
            for attempt in 1..=options.attempts {
                let output = virsh(
                    options,
                    repo,
                    &["dommemstat".to_owned(), options.vm.clone()],
                )?;
                validate_dommemstat(&output)?;
                println!(
                    "dommemstat fallback attempt {attempt}/{}: OK",
                    options.attempts
                );
            }
        }
    }
    println!("QEMU Guest Agent validation passed for {}.", options.vm);
    Ok(())
}

fn qga_command(options: &Options, repo: &Path, request: &str) -> Result<String, String> {
    virsh(
        options,
        repo,
        &[
            "qemu-agent-command".to_owned(),
            options.vm.clone(),
            request.to_owned(),
        ],
    )
}

fn virsh(options: &Options, repo: &Path, command: &[String]) -> Result<String, String> {
    let mut args = vec![OsString::from("-c"), OsString::from(&options.connect)];
    args.extend(command.iter().map(OsString::from));
    process::bounded_text(
        "virsh",
        &args,
        repo,
        Duration::from_secs(options.command_timeout_seconds),
    )
}

fn validate_memory_stats(input: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| format!("guest-get-memory-stats returned invalid JSON: {error}"))?;
    let values = value
        .get("return")
        .and_then(Value::as_array)
        .ok_or_else(|| "guest-get-memory-stats return must be an array".to_owned())?;
    let has_free = values
        .iter()
        .any(|entry| stat_is_numeric(entry, "stat-free"));
    let has_total = values
        .iter()
        .any(|entry| stat_is_numeric(entry, "stat-total"));
    if has_free && has_total {
        Ok(())
    } else {
        Err(
            "guest-get-memory-stats must contain numeric stat-free and stat-total values"
                .to_owned(),
        )
    }
}

fn stat_is_numeric(value: &Value, name: &str) -> bool {
    value.get("stat").and_then(Value::as_str) == Some(name)
        && value.get("value").and_then(Value::as_u64).is_some()
}

fn validate_dommemstat(input: &str) -> Result<(), String> {
    let has_numeric = |name: &str| {
        input.lines().any(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some(name)
                && fields
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some()
                && fields.next().is_none()
        })
    };
    if has_numeric("actual") && has_numeric("unused") && has_numeric("available") {
        Ok(())
    } else {
        Err("dommemstat did not return numeric actual, unused, and available fields".to_owned())
    }
}

fn valid_scope(value: &str) -> bool {
    !value.is_empty() && !value.starts_with('-') && !value.chars().any(char::is_control)
}

fn value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| format!("{option} requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_explicit_scope() {
        assert_eq!(
            parse(&strings(&[
                "guest",
                "--attempts",
                "4",
                "--command-timeout-seconds",
                "9",
                "--connect",
                "qemu:///session"
            ])),
            Ok(Options {
                vm: "guest".to_owned(),
                attempts: 4,
                connect: "qemu:///session".to_owned(),
                command_timeout_seconds: 9,
            })
        );
    }

    #[test]
    fn rejects_unsafe_or_incomplete_scope() {
        assert!(parse(&strings(&[])).is_err());
        assert!(parse(&strings(&["--guest"])).is_err());
        assert!(parse(&strings(&[
            "guest",
            "--attempts",
            "0",
            "--command-timeout-seconds",
            "9"
        ]))
        .is_err());
        assert!(parse(&strings(&[
            "guest",
            "--attempts",
            "1",
            "--command-timeout-seconds",
            "9",
            "--connect"
        ]))
        .is_err());
    }

    #[test]
    fn validates_qga_and_fallback_shapes() {
        assert!(validate_memory_stats(
            r#"{"return":[{"stat":"stat-free","value":1},{"stat":"stat-total","value":2}]}"#
        )
        .is_ok());
        assert!(validate_memory_stats(r#"{"return":[]}"#).is_err());
        assert!(validate_dommemstat("actual 1\nunused 2\navailable 3\n").is_ok());
        assert!(validate_dommemstat("actual 1\nunused 2\n").is_err());
        assert!(validate_dommemstat("actual x\nunused 2\navailable 3\n").is_err());
    }
}
