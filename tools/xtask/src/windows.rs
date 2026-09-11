use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::process;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Check,
    Sync,
    Build,
    Test,
    Lint,
    Fetch,
    All,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(Operation),
    Verify {
        expected_fingerprint: String,
        runs: u32,
    },
}

#[derive(Debug, Clone)]
struct Config {
    ssh_target: String,
    remote_dir: String,
    artifact_dir: PathBuf,
    known_hosts_file: Option<PathBuf>,
    identity_file: Option<PathBuf>,
    connect_timeout_seconds: u64,
    operation_timeout_seconds: u64,
}

#[derive(Debug)]
struct Toolchain {
    cargo: String,
    rustc: String,
    rustdoc: String,
    cargo_fmt: String,
    cargo_clippy: String,
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let operation = args.first().ok_or_else(|| {
        "windows requires check|sync|build|test|lint|fetch|all|verify".to_owned()
    })?;
    if operation == "verify" {
        if args.len() != 4 || args[2] != "--runs" {
            return Err("windows verify requires EXPECTED_ED25519_FINGERPRINT --runs N".to_owned());
        }
        let expected_fingerprint = args[1].clone();
        if !valid_fingerprint(&expected_fingerprint) {
            return Err("expected an OpenSSH SHA256 fingerprint".to_owned());
        }
        let runs = args[3]
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "--runs requires a positive integer".to_owned())?;
        return Ok(Command::Verify {
            expected_fingerprint,
            runs,
        });
    }
    if args.len() != 1 {
        return Err(format!(
            "windows {operation} does not accept positional arguments"
        ));
    }
    let operation = match operation.as_str() {
        "check" => Operation::Check,
        "sync" => Operation::Sync,
        "build" => Operation::Build,
        "test" => Operation::Test,
        "lint" => Operation::Lint,
        "fetch" => Operation::Fetch,
        "all" => Operation::All,
        _ => return Err(format!("unknown windows operation: {operation}")),
    };
    Ok(Command::Run(operation))
}

pub fn run(command: Command, repo: &Path) -> Result<(), String> {
    match command {
        Command::Run(operation) => run_operation(operation, &Config::from_env(repo)?, repo),
        Command::Verify {
            expected_fingerprint,
            runs,
        } => verify(repo, &Config::from_env(repo)?, &expected_fingerprint, runs),
    }
}

pub fn run_all(repo: &Path) -> Result<(), String> {
    run_operation(Operation::All, &Config::from_env(repo)?, repo)
}

impl Config {
    fn from_env(repo: &Path) -> Result<Self, String> {
        let ssh_target = std::env::var("VIRTIO_MEM_WINDOWS_SSH").map_err(|_| {
            "VIRTIO_MEM_WINDOWS_SSH is required; refusing to guess a guest".to_owned()
        })?;
        let remote_dir = required_env("VIRTIO_MEM_WINDOWS_DIR")?;
        let artifact_dir = PathBuf::from(required_env("VIRTIO_MEM_WINDOWS_ARTIFACTS")?);
        let artifact_dir = if artifact_dir.is_absolute() {
            artifact_dir
        } else {
            repo.join(artifact_dir)
        };
        let known_hosts_file = optional_path("VIRTIO_MEM_WINDOWS_KNOWN_HOSTS_FILE")?;
        let identity_file = optional_path("VIRTIO_MEM_WINDOWS_IDENTITY_FILE")?;
        let connect_timeout_seconds = positive_env("VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS")?;
        let operation_timeout_seconds =
            positive_env("VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS")?;
        let config = Self {
            ssh_target,
            remote_dir,
            artifact_dir,
            known_hosts_file,
            identity_file,
            connect_timeout_seconds,
            operation_timeout_seconds,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        validate_ssh_target(&self.ssh_target)?;
        if self.remote_dir.is_empty()
            || self.remote_dir.chars().any(char::is_control)
            || self
                .remote_dir
                .chars()
                .any(|character| "&|<>^%!\"".contains(character))
        {
            return Err("VIRTIO_MEM_WINDOWS_DIR contains characters unsafe for cmd.exe".to_owned());
        }
        Ok(())
    }

    fn ssh_options(&self) -> Vec<OsString> {
        let mut options = vec![
            process::os("-o"),
            process::os("BatchMode=yes"),
            process::os("-o"),
            process::os(format!(
                "ConnectTimeout={}",
                self.connect_timeout_seconds
            )),
        ];
        if let Some(path) = &self.known_hosts_file {
            options.extend([
                process::os("-o"),
                process::os(format!("UserKnownHostsFile={}", path.display())),
                process::os("-o"),
                process::os("StrictHostKeyChecking=yes"),
            ]);
        }
        if let Some(path) = &self.identity_file {
            options.extend([
                process::os("-o"),
                process::os("IdentitiesOnly=yes"),
                process::os("-i"),
                path.as_os_str().to_owned(),
            ]);
        }
        options
    }
}

fn run_operation(operation: Operation, config: &Config, repo: &Path) -> Result<(), String> {
    match operation {
        Operation::Check => check(config, repo),
        Operation::Sync => sync_source(config, repo),
        Operation::Build => build(config, repo),
        Operation::Test => test(config, repo),
        Operation::Lint => lint(config, repo),
        Operation::Fetch => fetch(config, repo),
        Operation::All => {
            check(config, repo)?;
            sync_source(config, repo)?;
            build(config, repo)?;
            test(config, repo)?;
            lint(config, repo)?;
            fetch(config, repo)
        }
    }
}

fn check(config: &Config, repo: &Path) -> Result<(), String> {
    require_commands(&["ssh", "scp", "tar", "sha256sum"])?;
    print_remote(
        config,
        repo,
        r#"where rustup && rustup --version && rustup target list --installed && rustup component list --installed && where tar && where certutil && if not exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" exit /b 1"#,
    )?;
    let toolchain = resolve_toolchain(config, repo)?;
    print_remote_msvc(
        config,
        repo,
        &format!(
            "\"{}\" --version ^&^& \"{}\" --version ^&^& \"{}\" clippy --version ^&^& \"{}\" fmt --version ^&^& where link",
            toolchain.rustc, toolchain.cargo, toolchain.cargo_clippy, toolchain.cargo_fmt
        ),
    )?;
    Ok(())
}

fn sync_source(config: &Config, repo: &Path) -> Result<(), String> {
    require_commands(&["git", "tar", "scp", "ssh"])?;
    let archive = std::env::temp_dir().join(format!(
        "virtio-mem-windows-source-{}.tar",
        std::process::id()
    ));
    let archive_text = archive
        .to_str()
        .ok_or_else(|| "temporary archive path is not valid UTF-8".to_owned())?;
    let result = (|| {
        let listed = process::checked_output(
            "git",
            &[
                process::os("ls-files"),
                process::os("--cached"),
                process::os("--others"),
                process::os("--exclude-standard"),
                process::os("-z"),
            ],
            repo,
        )?;
        let filtered = existing_paths(repo, &listed)?;
        process::run_with_input(
            "tar",
            &["--null", "--files-from=-", "-cf", archive_text],
            repo,
            &filtered,
        )?;
        let remote_archive = format!("virtio-mem-windows-source-{}.tar", std::process::id());
        print_remote(
            config,
            repo,
            &format!(
                "if not exist {} mkdir {}",
                quoted_remote_dir(config),
                quoted_remote_dir(config)
            ),
        )?;
        let mut scp_args = config.ssh_options();
        scp_args.extend([
            process::os("--"),
            archive.as_os_str().to_owned(),
            process::os(format!("{}:{remote_archive}", config.ssh_target)),
        ]);
        let output = process::bounded_output(
            "scp",
            &scp_args,
            repo,
            Duration::from_secs(config.operation_timeout_seconds),
        )?;
        print_checked(output, "scp source archive")?;
        print_remote(
            config,
            repo,
            &format!(
                "tar -xf {remote_archive} -C {} && del /q {remote_archive}",
                quoted_remote_dir(config)
            ),
        )?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&archive);
    result
}

fn build(config: &Config, repo: &Path) -> Result<(), String> {
    let tools = resolve_toolchain(config, repo)?;
    print_remote_msvc(
        config,
        repo,
        &format!(
            "set \"RUSTC={}\" ^&^& set \"RUSTDOC={}\" ^&^& \"{}\" build -p virtio-mem-service --release --locked",
            tools.rustc, tools.rustdoc, tools.cargo
        ),
    )?;
    Ok(())
}

fn test(config: &Config, repo: &Path) -> Result<(), String> {
    let tools = resolve_toolchain(config, repo)?;
    print_remote_msvc(
        config,
        repo,
        &format!(
            "set \"RUSTC={}\" ^&^& set \"RUSTDOC={}\" ^&^& \"{}\" test -p virtio-mem-service --all-features --locked",
            tools.rustc, tools.rustdoc, tools.cargo
        ),
    )?;
    Ok(())
}

fn lint(config: &Config, repo: &Path) -> Result<(), String> {
    let tools = resolve_toolchain(config, repo)?;
    print_remote_msvc(
        config,
        repo,
        &format!(
            "\"{}\" fmt --all -- --check ^&^& set \"RUSTC={}\" ^&^& set \"RUSTDOC={}\" ^&^& \"{}\" clippy -p virtio-mem-service --all-targets --all-features --locked -- -D warnings",
            tools.cargo_fmt, tools.rustc, tools.rustdoc, tools.cargo_clippy
        ),
    )?;
    Ok(())
}

fn fetch(config: &Config, repo: &Path) -> Result<(), String> {
    require_commands(&["scp", "ssh", "sha256sum"])?;
    std::fs::create_dir_all(&config.artifact_dir).map_err(|error| {
        format!(
            "failed to create artifact directory {}: {error}",
            config.artifact_dir.display()
        )
    })?;
    let destination = config.artifact_dir.join("virtio-mem-service.exe");
    let remote_artifact = format!(
        "{}/target/release/virtio-mem-service.exe",
        config.remote_dir.replace('\\', "/")
    );
    let mut scp_args = config.ssh_options();
    scp_args.extend([
        process::os("--"),
        process::os(format!("{}:{remote_artifact}", config.ssh_target)),
        destination.as_os_str().to_owned(),
    ]);
    print_checked(
        process::bounded_output(
            "scp",
            &scp_args,
            repo,
            Duration::from_secs(config.operation_timeout_seconds),
        )?,
        "scp Windows artifact",
    )?;
    let remote_hash_output = remote_stdout(
        config,
        repo,
        &format!(
            "certutil -hashfile {}\\target\\release\\virtio-mem-service.exe SHA256",
            quoted_remote_dir(config)
        ),
    )?;
    let remote_hash = parse_sha256(&remote_hash_output)
        .ok_or_else(|| "Windows certutil output did not contain a SHA-256 value".to_owned())?;
    let local_hash = sha256(repo, &destination)?;
    if remote_hash != local_hash {
        return Err(format!(
            "artifact checksum mismatch (Windows={remote_hash}, RHEL={local_hash})"
        ));
    }
    println!(
        "Windows artifact verified: {} ({local_hash})",
        destination.display()
    );
    Ok(())
}

fn resolve_toolchain(config: &Config, repo: &Path) -> Result<Toolchain, String> {
    let cargo = remote_stdout(config, repo, "rustup which cargo")?
        .replace('\r', "")
        .trim()
        .to_owned();
    let suffix = r"\bin\cargo.exe";
    let toolchain_bin = cargo
        .strip_suffix(suffix)
        .ok_or_else(|| format!("unable to resolve the active Windows Cargo toolchain: {cargo}"))?;
    Ok(Toolchain {
        cargo: cargo.clone(),
        rustc: format!(r"{toolchain_bin}\bin\rustc.exe"),
        rustdoc: format!(r"{toolchain_bin}\bin\rustdoc.exe"),
        cargo_fmt: format!(r"{toolchain_bin}\bin\cargo-fmt.exe"),
        cargo_clippy: format!(r"{toolchain_bin}\bin\cargo-clippy.exe"),
    })
}

fn print_remote_msvc(config: &Config, repo: &Path, command: &str) -> Result<String, String> {
    print_remote(
        config,
        repo,
        &format!(
            "for /f \"delims=\" %i in ('\"%ProgramFiles(x86)%\\Microsoft Visual Studio\\Installer\\vswhere.exe\" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find VC\\Auxiliary\\Build\\vcvars64.bat') do @call \"%i\" ^&^& cd /d {} ^&^& {command}",
            quoted_remote_dir(config)
        ),
    )
}

fn print_remote(config: &Config, repo: &Path, command: &str) -> Result<String, String> {
    let output = remote_output(config, repo, command)?;
    let stdout = String::from_utf8(output.stdout.clone())
        .map_err(|error| format!("ssh returned invalid UTF-8 on stdout: {error}"))?;
    print_checked(output, "ssh remote command")?;
    Ok(stdout)
}

fn remote_stdout(config: &Config, repo: &Path, command: &str) -> Result<String, String> {
    let output = remote_output(config, repo, command)?;
    if !output.status.success() {
        return Err(format!(
            "ssh remote command failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("ssh returned invalid UTF-8 on stdout: {error}"))
}

fn remote_output(
    config: &Config,
    repo: &Path,
    command: &str,
) -> Result<std::process::Output, String> {
    let mut args = config.ssh_options();
    args.extend([
        process::os("--"),
        process::os(&config.ssh_target),
        process::os(format!("cmd.exe /d /c {command}")),
    ]);
    process::bounded_output(
        "ssh",
        &args,
        repo,
        Duration::from_secs(config.operation_timeout_seconds),
    )
}

fn quoted_remote_dir(config: &Config) -> String {
    format!("\"{}\"", config.remote_dir)
}

fn verify(
    repo: &Path,
    config: &Config,
    expected_fingerprint: &str,
    runs: u32,
) -> Result<(), String> {
    require_commands(&["ssh", "ssh-keygen", "ssh-keyscan", "sha256sum"])?;
    let timeout = Duration::from_secs(config.operation_timeout_seconds);
    let ssh_config = process::bounded_text(
        "ssh",
        &[
            process::os("-G"),
            process::os("--"),
            process::os(&config.ssh_target),
        ],
        repo,
        timeout,
    )?;
    let host_name = ssh_config_value(&ssh_config, "hostname")
        .ok_or_else(|| "could not resolve SSH hostname".to_owned())?;
    let host_port = ssh_config_value(&ssh_config, "port")
        .ok_or_else(|| "could not resolve SSH port".to_owned())?;
    let known_hosts =
        std::env::temp_dir().join(format!("virtio-mem-known-hosts-{}", std::process::id()));
    let result = (|| {
        let scan = process::bounded_text(
            "ssh-keyscan",
            &[
                process::os("-T"),
                process::os(config.connect_timeout_seconds.to_string()),
                process::os("-p"),
                process::os(&host_port),
                process::os("-t"),
                process::os("ed25519"),
                process::os("--"),
                process::os(&host_name),
            ],
            repo,
            timeout,
        )?;
        std::fs::write(&known_hosts, scan)
            .map_err(|error| format!("failed to write pinned known-hosts file: {error}"))?;
        let fingerprint_output = process::bounded_text(
            "ssh-keygen",
            &[
                process::os("-E"),
                process::os("sha256"),
                process::os("-lf"),
                known_hosts.as_os_str().to_owned(),
            ],
            repo,
            timeout,
        )?;
        let observed = fingerprint_output
            .split_whitespace()
            .find(|field| field.starts_with("SHA256:"))
            .ok_or_else(|| "ssh-keygen did not return a SHA-256 fingerprint".to_owned())?;
        if observed != expected_fingerprint {
            return Err(format!(
                "Windows SSH host fingerprint mismatch (expected={expected_fingerprint}, observed={observed})"
            ));
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_millis();
        let evidence_dir = config
            .artifact_dir
            .join(format!("verification-{timestamp}"));
        std::fs::create_dir_all(&evidence_dir)
            .map_err(|error| format!("failed to create {}: {error}", evidence_dir.display()))?;
        let summary = evidence_dir.join("summary.txt");
        append_summary(
            &summary,
            &format!(
                "Unix run timestamp (milliseconds): {timestamp}\nWindows SSH target: {}\nVerified Windows SSH host fingerprint: {observed}\nConfigured aggregate runs: {runs}\nEvidence directory: {}\n",
                config.ssh_target,
                evidence_dir.display()
            ),
        )?;
        println!(
            "Verified endpoint; evidence directory: {}",
            evidence_dir.display()
        );

        let mut verified_config = config.clone();
        verified_config.known_hosts_file = Some(known_hosts.clone());
        for run_number in 1..=runs {
            println!("Starting aggregate gate {run_number} of {runs}.");
            crate::local::run(crate::local::Step::Local, repo)?;
            run_operation(Operation::All, &verified_config, repo)?;
            let artifact = config.artifact_dir.join("virtio-mem-service.exe");
            let hash = sha256(repo, &artifact)?;
            std::fs::write(
                evidence_dir.join(format!("artifact-run-{run_number}.sha256")),
                format!("{hash}  {}\n", artifact.display()),
            )
            .map_err(|error| format!("failed to write artifact hash: {error}"))?;
        }
        append_summary(&summary, &format!("{runs} configured aggregate gate run(s) passed.\n"))?;
        println!("{runs} configured aggregate gate run(s) passed.");
        Ok(())
    })();
    let _ = std::fs::remove_file(&known_hosts);
    result
}

fn append_summary(path: &Path, value: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    file.write_all(value.as_bytes())
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn positive_env(name: &str) -> Result<u64, String> {
    let value = required_env(name)?;
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive integer"))
}

fn optional_path(name: &str) -> Result<Option<PathBuf>, String> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(value);
    if !path.is_file() {
        return Err(format!("{name} does not exist: {}", path.display()));
    }
    Ok(Some(path))
}

fn require_commands(commands: &[&str]) -> Result<(), String> {
    let missing: Vec<_> = commands
        .iter()
        .copied()
        .filter(|command| !process::command_exists(command))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "missing required command(s): {}",
            missing.join(", ")
        ))
    }
}

fn validate_ssh_target(value: &str) -> Result<(), String> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        Err("SSH target must be an alias or USER@HOST, not an option or command".to_owned())
    } else {
        Ok(())
    }
}

fn valid_fingerprint(value: &str) -> bool {
    value
        .strip_prefix("SHA256:")
        .filter(|digest| !digest.is_empty())
        .is_some_and(|digest| {
            digest
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "+/=".contains(character))
        })
}

fn ssh_config_value(input: &str, key: &str) -> Option<String> {
    input.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some(key))
            .then(|| fields.next().map(str::to_owned))
            .flatten()
    })
}

fn parse_sha256(input: &str) -> Option<String> {
    input
        .lines()
        .map(str::trim)
        .find(|line| {
            line.len() == 64 && line.chars().all(|character| character.is_ascii_hexdigit())
        })
        .map(str::to_ascii_lowercase)
}

fn existing_paths(repo: &Path, nul_separated: &[u8]) -> Result<Vec<u8>, String> {
    let input = std::str::from_utf8(nul_separated)
        .map_err(|error| format!("git returned a non-UTF-8 path: {error}"))?;
    let mut output = Vec::with_capacity(nul_separated.len());
    for relative in input.split('\0').filter(|path| !path.is_empty()) {
        if std::fs::symlink_metadata(repo.join(relative)).is_ok() {
            output.extend_from_slice(relative.as_bytes());
            output.push(0);
        }
    }
    if output.is_empty() {
        return Err("Git did not report any existing source files to synchronize".to_owned());
    }
    Ok(output)
}

fn sha256(repo: &Path, path: &Path) -> Result<String, String> {
    let output = process::checked_text("sha256sum", &[path.as_os_str().to_owned()], repo)?;
    output
        .split_whitespace()
        .next()
        .filter(|value| value.len() == 64)
        .map(str::to_owned)
        .ok_or_else(|| "sha256sum returned malformed output".to_owned())
}

fn print_checked(output: std::process::Output, label: &str) -> Result<(), String> {
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with status {}", output.status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_operations_and_explicit_verification_count() {
        assert_eq!(parse(&strings(&["all"])), Ok(Command::Run(Operation::All)));
        assert!(matches!(
            parse(&strings(&["verify", "SHA256:abc+", "--runs", "2"])),
            Ok(Command::Verify { runs: 2, .. })
        ));
    }

    #[test]
    fn rejects_unsafe_remote_inputs() {
        assert!(parse(&strings(&["unknown"])).is_err());
        assert!(parse(&strings(&["verify", "md5:abc", "--runs", "1"])).is_err());
        assert!(parse(&strings(&["verify", "SHA256:abc", "--runs", "0"])).is_err());
    }

    #[test]
    fn parses_tool_outputs_without_accepting_noise() {
        assert_eq!(
            parse_sha256("SHA256 hash of file:\r\nABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789\r\n"),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_owned())
        );
        assert_eq!(
            ssh_config_value("hostname host.example\nport 22\n", "port"),
            Some("22".to_owned())
        );
    }

    #[test]
    fn filters_working_tree_deletions_from_git_inventory() {
        let root =
            std::env::temp_dir().join(format!("virtio-mem-xtask-sync-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create fixture directory");
        std::fs::write(root.join("present"), "fixture").expect("write fixture");
        let filtered = existing_paths(&root, b"present\0deleted\0").expect("filter paths");
        std::fs::remove_file(root.join("present")).expect("remove fixture file");
        std::fs::remove_dir(root).expect("remove fixture directory");
        assert_eq!(filtered, b"present\0");
    }
}
