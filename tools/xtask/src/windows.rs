use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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
    Deploy {
        manifest: PathBuf,
        output: PathBuf,
        apply: bool,
    },
    ServiceCycle {
        service: String,
        output: PathBuf,
        apply: bool,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentManifest {
    schema_version: u32,
    vm_name: String,
    service_name: String,
    display_name: String,
    description: String,
    telemetry_path: String,
    service_account: String,
    poll_interval_millis: u64,
    shutdown_timeout_millis: u64,
    install_path: String,
}

#[derive(Debug, Serialize)]
struct DeploymentEvidence {
    schema_version: u32,
    applied: bool,
    source_revision: String,
    candidate_sha256: String,
    manifest: DeploymentManifestEvidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    windows: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct DeploymentManifestEvidence {
    vm_name: String,
    service_name: String,
    telemetry_path: String,
    service_account: String,
    poll_interval_millis: u64,
    shutdown_timeout_millis: u64,
    install_path: String,
}

#[derive(Debug, Clone)]
struct Config {
    ssh_target: String,
    remote_dir: String,
    artifact_dir: PathBuf,
    known_hosts_file: Option<PathBuf>,
    identity_file: Option<PathBuf>,
    host_name: Option<String>,
    host_key_alias: Option<String>,
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
        "windows requires check|sync|build|test|lint|fetch|all|verify|deploy".to_owned()
    })?;
    if operation == "deploy" {
        if args.len() != 4 && args.len() != 5 {
            return Err("windows deploy requires MANIFEST --output PATH [--apply]".to_owned());
        }
        if args[2] != "--output" || args[3].trim().is_empty() {
            return Err("windows deploy requires MANIFEST --output PATH [--apply]".to_owned());
        }
        let apply = args.get(4).map(String::as_str) == Some("--apply");
        if args.len() == 5 && !apply {
            return Err(format!("unknown windows deploy option: {}", args[4]));
        }
        return Ok(Command::Deploy {
            manifest: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[3]),
            apply,
        });
    }
    if operation == "service-cycle" {
        if (args.len() != 4 && args.len() != 5)
            || args[2] != "--output"
            || args[1].trim().is_empty()
            || args[3].trim().is_empty()
        {
            return Err(
                "windows service-cycle requires SERVICE --output PATH [--apply]".to_owned(),
            );
        }
        validate_ssh_target(&args[1])?;
        let apply = args.get(4).map(String::as_str) == Some("--apply");
        if args.len() == 5 && !apply {
            return Err(format!("unknown windows service-cycle option: {}", args[4]));
        }
        return Ok(Command::ServiceCycle {
            service: args[1].clone(),
            output: PathBuf::from(&args[3]),
            apply,
        });
    }
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
        Command::Deploy {
            manifest,
            output,
            apply,
        } => deploy(repo, &Config::from_env(repo)?, &manifest, &output, apply),
        Command::ServiceCycle {
            service,
            output,
            apply,
        } => service_cycle(repo, &Config::from_env(repo)?, &service, &output, apply),
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
        let host_name = optional_env("VIRTIO_MEM_WINDOWS_HOST_NAME");
        let host_key_alias = optional_env("VIRTIO_MEM_WINDOWS_HOST_KEY_ALIAS");
        let connect_timeout_seconds = positive_env("VIRTIO_MEM_WINDOWS_CONNECT_TIMEOUT_SECONDS")?;
        let operation_timeout_seconds =
            positive_env("VIRTIO_MEM_WINDOWS_OPERATION_TIMEOUT_SECONDS")?;
        let config = Self {
            ssh_target,
            remote_dir,
            artifact_dir,
            known_hosts_file,
            identity_file,
            host_name,
            host_key_alias,
            connect_timeout_seconds,
            operation_timeout_seconds,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        validate_ssh_target(&self.ssh_target)?;
        if self.host_name.is_some() != self.host_key_alias.is_some() {
            return Err("VIRTIO_MEM_WINDOWS_HOST_NAME and VIRTIO_MEM_WINDOWS_HOST_KEY_ALIAS must be supplied together".to_owned());
        }
        if let Some(value) = &self.host_name {
            validate_ssh_target(value)?;
        }
        if let Some(value) = &self.host_key_alias {
            validate_ssh_target(value)?;
        }
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
            process::os(format!("ConnectTimeout={}", self.connect_timeout_seconds)),
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
        if let (Some(host_name), Some(host_key_alias)) = (&self.host_name, &self.host_key_alias) {
            options.extend([
                process::os("-o"),
                process::os(format!("HostName={host_name}")),
                process::os("-o"),
                process::os(format!("HostKeyAlias={host_key_alias}")),
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
        append_summary(
            &summary,
            &format!("{runs} configured aggregate gate run(s) passed.\n"),
        )?;
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

fn deploy(
    repo: &Path,
    config: &Config,
    manifest_path: &Path,
    output_path: &Path,
    apply: bool,
) -> Result<(), String> {
    let manifest_path = resolve_local(repo, manifest_path);
    let output_path = resolve_local(repo, output_path);
    let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "read Windows deployment manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let manifest: DeploymentManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("parse Windows deployment manifest: {error}"))?;
    validate_deployment_manifest(&manifest)?;

    let candidate_path = format!(
        r"{}\target\release\virtio-mem-service.exe",
        config.remote_dir.trim_end_matches(['\\', '/'])
    );
    let candidate_hash_output = remote_stdout(
        config,
        repo,
        &format!("certutil -hashfile \"{candidate_path}\" SHA256"),
    )?;
    let candidate_sha256 = parse_sha256(&candidate_hash_output)
        .ok_or_else(|| "Windows certutil output did not contain a candidate SHA-256".to_owned())?;
    let source_revision = process::bounded_text(
        "git",
        &[process::os("rev-parse"), process::os("HEAD")],
        repo,
        Duration::from_secs(config.operation_timeout_seconds),
    )?
    .trim()
    .to_owned();

    let windows = if apply {
        let service_config = serde_json::json!({
            "schema_version": 4,
            "vm_name": manifest.vm_name,
            "service_name": manifest.service_name,
            "display_name": manifest.display_name,
            "description": manifest.description,
            "demand_report_path": manifest.telemetry_path,
            "service_account": manifest.service_account,
            "poll_interval_millis": manifest.poll_interval_millis,
            "shutdown_timeout_millis": manifest.shutdown_timeout_millis,
        });
        let config_base64 = base64_encode(
            serde_json::to_string_pretty(&service_config)
                .map_err(|error| format!("encode Windows service configuration: {error}"))?
                .as_bytes(),
        );
        let script = deployment_powershell(&manifest, &candidate_path, &config_base64);
        let output = remote_powershell(config, repo, &script)?;
        Some(parse_last_json_line(&output)?)
    } else {
        None
    };

    let evidence = DeploymentEvidence {
        schema_version: 1,
        applied: apply,
        source_revision,
        candidate_sha256,
        manifest: DeploymentManifestEvidence {
            vm_name: manifest.vm_name,
            service_name: manifest.service_name,
            telemetry_path: manifest.telemetry_path,
            service_account: manifest.service_account,
            poll_interval_millis: manifest.poll_interval_millis,
            shutdown_timeout_millis: manifest.shutdown_timeout_millis,
            install_path: manifest.install_path,
        },
        windows,
    };
    persist_json(&output_path, &evidence)?;
    println!(
        "Windows deployment {} evidence written to {}",
        if apply { "apply" } else { "dry-run" },
        output_path.display()
    );
    Ok(())
}

fn service_cycle(
    repo: &Path,
    config: &Config,
    service: &str,
    output: &Path,
    apply: bool,
) -> Result<(), String> {
    let service = ps_literal(service);
    let action = if apply {
        "if($before.State -ne 'Stopped'){Stop-Service -Name $service -Force; (Get-Service $service).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30))}; Start-Service -Name $service; (Get-Service $service).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))"
    } else {
        ""
    };
    let script = format!(
        r#"$ErrorActionPreference='Stop'
$service={service}
$before=Get-CimInstance Win32_Service -Filter ("Name='"+$service.Replace("'","''")+"'")
if($null -eq $before){{throw 'named Windows service was not found'}}
try {{{action}}} catch {{if($before.State -eq 'Running'){{Start-Service -Name $service -ErrorAction SilentlyContinue}}; throw}}
$after=Get-CimInstance Win32_Service -Filter ("Name='"+$service.Replace("'","''")+"'")
[ordered]@{{captured_unix_millis=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();applied={apply};service_name=$after.Name;display_name=$after.DisplayName;before_state=$before.State;after_state=$after.State;start_mode=$after.StartMode;account=$after.StartName;path=$after.PathName}} | ConvertTo-Json -Compress
"#,
        apply = if apply { "$true" } else { "$false" },
    );
    let value = parse_last_json_line(&remote_powershell(config, repo, &script)?)?;
    let output = resolve_local(repo, output);
    persist_json(&output, &value)?;
    println!(
        "Windows service-cycle {} evidence written to {}",
        if apply { "apply" } else { "dry-run" },
        output.display()
    );
    Ok(())
}

fn parse_last_json_line(output: &str) -> Result<serde_json::Value, String> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find_map(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| {
            let bounded = output.chars().take(1_024).collect::<String>();
            format!("Windows deployment did not return JSON evidence: {bounded:?}")
        })
}

fn validate_deployment_manifest(manifest: &DeploymentManifest) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported Windows deployment manifest schema version: {}",
            manifest.schema_version
        ));
    }
    for (name, value) in [
        ("vm_name", &manifest.vm_name),
        ("service_name", &manifest.service_name),
        ("display_name", &manifest.display_name),
        ("description", &manifest.description),
        ("telemetry_path", &manifest.telemetry_path),
        ("service_account", &manifest.service_account),
        ("install_path", &manifest.install_path),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(format!(
                "Windows deployment manifest {name} is empty or unsafe"
            ));
        }
    }
    for (name, value) in [
        ("telemetry_path", &manifest.telemetry_path),
        ("install_path", &manifest.install_path),
    ] {
        let bytes = value.as_bytes();
        if bytes.len() < 4
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
            || value.contains("..")
        {
            return Err(format!(
                "Windows deployment manifest {name} must be an absolute drive path without '..'"
            ));
        }
    }
    if !manifest.install_path.to_ascii_lowercase().ends_with(".exe") {
        return Err("Windows deployment install_path must name an .exe file".to_owned());
    }
    if manifest.poll_interval_millis == 0 || manifest.shutdown_timeout_millis == 0 {
        return Err("Windows deployment timings must be positive".to_owned());
    }
    Ok(())
}

fn deployment_powershell(
    manifest: &DeploymentManifest,
    candidate_path: &str,
    config_base64: &str,
) -> String {
    let service = ps_literal(&manifest.service_name);
    let candidate = ps_literal(candidate_path);
    let install = ps_literal(&manifest.install_path);
    let telemetry = ps_literal(&manifest.telemetry_path);
    let wait_millis = manifest.poll_interval_millis.saturating_mul(3).max(1000);
    format!(
        r#"$ErrorActionPreference='Stop'
$service={service}
$candidate={candidate}
$install={install}
$telemetry={telemetry}
$config='C:\ProgramData\VirtioMemService\config.json'
$root='C:\ProgramData\VirtioMemService'
$backup=Join-Path $root ('deployment-backups\'+[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
$prior=Get-CimInstance Win32_Service -Filter ("Name='"+$service.Replace("'","''")+"'") -ErrorAction SilentlyContinue
$priorRunning=($null -ne $prior -and $prior.State -eq 'Running')
New-Item -ItemType Directory -Force -Path $backup | Out-Null
if(Test-Path $install){{Copy-Item -LiteralPath $install -Destination (Join-Path $backup 'service.exe') -Force}}
if(Test-Path $config){{Copy-Item -LiteralPath $config -Destination (Join-Path $backup 'config.json') -Force}}
if($null -ne $prior){{& reg.exe export ('HKLM\SYSTEM\CurrentControlSet\Services\'+$service) (Join-Path $backup 'service.reg') /y | Out-Null}}
try {{
  if($null -ne $prior){{if($prior.State -ne 'Stopped'){{Stop-Service -Name $service -Force; (Get-Service $service).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30))}}; & sc.exe delete $service | Out-Null; Start-Sleep -Milliseconds 500}}
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $install),$root | Out-Null
  Copy-Item -LiteralPath $candidate -Destination $install -Force
  [IO.File]::WriteAllBytes($config,[Convert]::FromBase64String('{config_base64}'))
  & $install install
  if($LASTEXITCODE -ne 0){{throw "candidate install failed with exit code $LASTEXITCODE"}}
  & $install start
  if($LASTEXITCODE -ne 0){{throw "candidate start failed with exit code $LASTEXITCODE"}}
  (Get-Service $service).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))
  Start-Sleep -Milliseconds {wait_millis}
  $first=Get-Content -LiteralPath $telemetry -Raw | ConvertFrom-Json
  Start-Sleep -Milliseconds {wait_millis}
  $second=Get-Content -LiteralPath $telemetry -Raw | ConvertFrom-Json
  if($second.sequence -le $first.sequence){{throw 'telemetry sequence did not advance'}}
  if($second.vm_name -ne {vm} -or $second.service_name -ne $service){{throw 'telemetry identity mismatch'}}
  $installed=Get-CimInstance Win32_Service -Filter ("Name='"+$service.Replace("'","''")+"'")
  [ordered]@{{
    captured_unix_millis=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    backup_path=$backup
    binary_sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $install).Hash.ToLowerInvariant()
    config_sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $config).Hash.ToLowerInvariant()
    binary_acl=(Get-Acl -LiteralPath $install).Sddl
    config_acl=(Get-Acl -LiteralPath $config).Sddl
    telemetry_acl=(Get-Acl -LiteralPath $telemetry).Sddl
    service_name=$installed.Name
    service_state=$installed.State
    service_start_mode=$installed.StartMode
    service_account=$installed.StartName
    service_path=$installed.PathName
    service_error_control=$installed.ErrorControl
    first_session_id=$first.session_id
    first_sequence=$first.sequence
    second_session_id=$second.session_id
    second_sequence=$second.sequence
  }} | ConvertTo-Json -Compress
}} catch {{
  try {{Stop-Service -Name $service -Force -ErrorAction SilentlyContinue; & sc.exe delete $service | Out-Null}} catch {{}}
  if(Test-Path (Join-Path $backup 'service.exe')){{Copy-Item -LiteralPath (Join-Path $backup 'service.exe') -Destination $install -Force}}
  if(Test-Path (Join-Path $backup 'config.json')){{Copy-Item -LiteralPath (Join-Path $backup 'config.json') -Destination $config -Force}} else {{Remove-Item -LiteralPath $config -Force -ErrorAction SilentlyContinue}}
  if(Test-Path (Join-Path $backup 'service.reg')){{& reg.exe import (Join-Path $backup 'service.reg') | Out-Null; if($priorRunning){{Start-Service -Name $service}}}}
  throw
}}"#,
        vm = ps_literal(&manifest.vm_name),
    )
}

fn remote_powershell(config: &Config, repo: &Path, script: &str) -> Result<String, String> {
    let nonce = format!("{}-{}", std::process::id(), unix_millis()?);
    let local = std::env::temp_dir().join(format!("virtio-mem-deploy-{nonce}.ps1"));
    let remote = format!("virtio-mem-deploy-{nonce}.ps1");
    std::fs::write(&local, script)
        .map_err(|error| format!("write temporary PowerShell payload: {error}"))?;
    let result = (|| {
        let mut copy_args = config.ssh_options();
        copy_args.extend([
            process::os("--"),
            local.as_os_str().to_owned(),
            process::os(format!("{}:{remote}", config.ssh_target)),
        ]);
        let copy = process::bounded_output(
            "scp",
            &copy_args,
            repo,
            Duration::from_secs(config.operation_timeout_seconds),
        )?;
        if !copy.status.success() {
            return Err(format!(
                "copy temporary PowerShell payload failed with status {}: {}",
                copy.status,
                String::from_utf8_lossy(&copy.stderr).trim()
            ));
        }
        remote_stdout(
            config,
            repo,
            &format!("powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File {remote}"),
        )
    })();
    let _ = remote_output(config, repo, &format!("del /q {remote}"));
    let _ = std::fs::remove_file(local);
    result
}

fn unix_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

fn ps_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn resolve_local(repo: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

fn persist_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("deployment evidence path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("create deployment evidence directory: {error}"))?;
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("serialize deployment evidence: {error}"))?;
    bytes.push(b'\n');
    let temporary = parent.join(format!(".windows-deployment-{}.tmp", std::process::id()));
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("write deployment evidence: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("commit deployment evidence: {error}"))
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

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
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
        assert!(matches!(
            parse(&strings(&[
                "deploy",
                "deployment.json",
                "--output",
                "evidence.json",
                "--apply"
            ])),
            Ok(Command::Deploy { apply: true, .. })
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

    #[test]
    fn deployment_manifest_requires_explicit_safe_paths_and_timings() {
        let mut manifest = DeploymentManifest {
            schema_version: 1,
            vm_name: "guest".to_owned(),
            service_name: "VirtioMemService".to_owned(),
            display_name: "Virtio memory telemetry".to_owned(),
            description: "Publishes telemetry".to_owned(),
            telemetry_path: r"C:\ProgramData\VirtioMemService\telemetry.json".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            poll_interval_millis: 5_000,
            shutdown_timeout_millis: 30_000,
            install_path: r"C:\Program Files\VirtioMemService\virtio-mem-service.exe".to_owned(),
        };
        assert!(validate_deployment_manifest(&manifest).is_ok());
        manifest.telemetry_path = r"relative\telemetry.json".to_owned();
        assert!(validate_deployment_manifest(&manifest).is_err());
        manifest.telemetry_path = r"C:\ProgramData\..\unsafe.json".to_owned();
        assert!(validate_deployment_manifest(&manifest).is_err());
    }

    #[test]
    fn powershell_payload_encoding_is_standard_base64() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(ps_literal("a'b"), "'a''b'");
        assert_eq!(
            parse_last_json_line("service installed\n{\"ok\":true}\n").expect("last JSON line")
                ["ok"],
            true
        );
    }
}
