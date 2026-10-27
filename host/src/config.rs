use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

/// Unplug must be qualified on the deployment before automatic reclaim is enabled.
pub const DEFAULT_AUTOMATIC_WINDOWS_SHRINK: bool = false;

/// Bounds a configuration read; a real file is a few kilobytes.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostConfigError {
    #[error("read host config {path}: {message}")]
    Read { path: String, message: String },
    #[error("host config is invalid TOML or has unknown keys: {0}")]
    Parse(String),
    #[error("host config must list at least one [[vm]]")]
    NoVm,
    #[error("host config lists {0} VMs; the shared pool is not wired yet, so configure exactly one [[vm]]")]
    PoolNotWired(usize),
    #[error("vm {0} is configured more than once")]
    DuplicateVm(String),
    #[error("policy_state_path {0} is shared by more than one VM")]
    SharedPolicyState(String),
    #[error("vm {vm}: {source}")]
    Vm {
        vm: String,
        #[source]
        source: Box<HostConfigError>,
    },
    #[error("{0} must be non-empty")]
    Empty(&'static str),
    #[error("priority must be greater than zero")]
    InvalidPriority,
    #[error("alias contains unsupported characters")]
    InvalidAlias,
    #[error("minimum memory must not exceed maximum memory")]
    InvalidMemoryRange,
    #[error("grow and shrink steps must be greater than zero")]
    InvalidResizeStep,
    #[error("host-controller durations must be greater than zero")]
    InvalidDuration,
    #[error("safe-floor reserves must not exceed normal reserves")]
    InvalidReserveOrder,
    #[error("target policy configuration is invalid: {0}")]
    InvalidTargetPolicy(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RawTelemetryTransport {
    File,
    QgaFile,
}

/// One VM's resolved settings: the file's global keys plus its `[[vm]]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub vm_name: String,
    pub alias: String,
    /// Pool priority; larger is served first. Used once the pool is wired.
    pub priority: u32,
    pub min_memory_bytes: u64,
    pub max_memory_bytes: u64,
    pub grow_step_bytes: u64,
    pub shrink_step_bytes: u64,
    pub fixed_visible_base_bytes: u64,
    pub physical_reserve_bytes: u64,
    pub commit_reserve_bytes: u64,
    pub safe_floor_physical_reserve_bytes: u64,
    pub safe_floor_commit_reserve_bytes: u64,
    pub reclaim_history: Duration,
    pub reclaim_max_gap: Duration,
    pub downward_hysteresis_bytes: u64,
    pub policy_state_path: String,
    pub poll_interval: Duration,
    pub command_timeout: Duration,
    pub convergence_timeout: Duration,
    pub virsh_binary: String,
    pub raw_telemetry_path: String,
    pub raw_telemetry_transport: RawTelemetryTransport,
    pub raw_telemetry_service_name: String,
    pub raw_telemetry_max_age: Duration,
    pub raw_telemetry_future_tolerance: Duration,
    pub host_min_headroom_bytes: u64,
    pub compatibility_attestation_path: String,
    pub automatic_windows_shrink: bool,
    /// Evaluate and log decisions but never request a resize.
    pub observe_only: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    poll_interval_seconds: u64,
    command_timeout_seconds: u64,
    convergence_timeout_seconds: u64,
    host_min_headroom_bytes: u64,
    #[serde(default = "default_virsh_binary")]
    virsh_binary: String,
    #[serde(default)]
    observe_only: bool,
    #[serde(default, rename = "vm")]
    vms: Vec<VmTable>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VmTable {
    name: String,
    alias: String,
    priority: u32,
    min_memory_bytes: u64,
    max_memory_bytes: u64,
    grow_step_bytes: u64,
    shrink_step_bytes: u64,
    fixed_visible_base_bytes: u64,
    physical_reserve_bytes: u64,
    commit_reserve_bytes: u64,
    safe_floor_physical_reserve_bytes: u64,
    safe_floor_commit_reserve_bytes: u64,
    reclaim_history_seconds: u64,
    /// Derived from the polling interval when absent.
    reclaim_max_gap_seconds: Option<u64>,
    downward_hysteresis_bytes: u64,
    policy_state_path: String,
    compatibility_attestation_path: String,
    #[serde(default = "default_automatic_windows_shrink")]
    automatic_windows_shrink: bool,
    telemetry_transport: RawTelemetryTransport,
    telemetry_path: String,
    telemetry_service_name: String,
    telemetry_max_age_seconds: u64,
    telemetry_future_tolerance_seconds: u64,
}

fn default_virsh_binary() -> String {
    "virsh".to_owned()
}

fn default_automatic_windows_shrink() -> bool {
    DEFAULT_AUTOMATIC_WINDOWS_SHRINK
}

impl HostConfig {
    /// Load every configured VM. Each is validated, and VM names and policy
    /// state paths must be unique so two controllers never share state.
    pub fn load(path: &Path) -> Result<Vec<Self>, HostConfigError> {
        let read_error = |message: String| HostConfigError::Read {
            path: path.display().to_string(),
            message,
        };
        let length = std::fs::metadata(path)
            .map_err(|error| read_error(error.to_string()))?
            .len();
        if length > MAX_CONFIG_BYTES {
            return Err(read_error(format!("larger than {MAX_CONFIG_BYTES} bytes")));
        }
        let text = std::fs::read_to_string(path).map_err(|error| read_error(error.to_string()))?;
        Self::parse(&text)
    }

    /// Load the one VM the controller serves until the shared pool is wired.
    pub fn load_single(path: &Path) -> Result<Self, HostConfigError> {
        let mut vms = Self::load(path)?;
        match vms.len() {
            1 => Ok(vms.remove(0)),
            count => Err(HostConfigError::PoolNotWired(count)),
        }
    }

    pub fn parse(text: &str) -> Result<Vec<Self>, HostConfigError> {
        let file: ConfigFile =
            toml::from_str(text).map_err(|error| HostConfigError::Parse(error.to_string()))?;
        if file.vms.is_empty() {
            return Err(HostConfigError::NoVm);
        }
        let mut names = BTreeSet::new();
        let mut state_paths = BTreeSet::new();
        let mut configs = Vec::with_capacity(file.vms.len());
        for vm in &file.vms {
            let config = Self::resolve(&file, vm).map_err(|source| HostConfigError::Vm {
                vm: vm.name.clone(),
                source: Box::new(source),
            })?;
            if !names.insert(config.vm_name.clone()) {
                return Err(HostConfigError::DuplicateVm(config.vm_name));
            }
            if !state_paths.insert(config.policy_state_path.clone()) {
                return Err(HostConfigError::SharedPolicyState(config.policy_state_path));
            }
            configs.push(config);
        }
        Ok(configs)
    }

    fn resolve(file: &ConfigFile, vm: &VmTable) -> Result<Self, HostConfigError> {
        let reclaim_max_gap_seconds = match vm.reclaim_max_gap_seconds {
            Some(seconds) => seconds,
            None => file.poll_interval_seconds.checked_mul(2).ok_or(
                HostConfigError::InvalidTargetPolicy("derived reclaim maximum gap overflowed"),
            )?,
        };
        let config = Self {
            vm_name: vm.name.clone(),
            alias: vm.alias.clone(),
            priority: vm.priority,
            min_memory_bytes: vm.min_memory_bytes,
            max_memory_bytes: vm.max_memory_bytes,
            grow_step_bytes: vm.grow_step_bytes,
            shrink_step_bytes: vm.shrink_step_bytes,
            fixed_visible_base_bytes: vm.fixed_visible_base_bytes,
            physical_reserve_bytes: vm.physical_reserve_bytes,
            commit_reserve_bytes: vm.commit_reserve_bytes,
            safe_floor_physical_reserve_bytes: vm.safe_floor_physical_reserve_bytes,
            safe_floor_commit_reserve_bytes: vm.safe_floor_commit_reserve_bytes,
            reclaim_history: Duration::from_secs(vm.reclaim_history_seconds),
            reclaim_max_gap: Duration::from_secs(reclaim_max_gap_seconds),
            downward_hysteresis_bytes: vm.downward_hysteresis_bytes,
            policy_state_path: vm.policy_state_path.clone(),
            poll_interval: Duration::from_secs(file.poll_interval_seconds),
            command_timeout: Duration::from_secs(file.command_timeout_seconds),
            convergence_timeout: Duration::from_secs(file.convergence_timeout_seconds),
            virsh_binary: file.virsh_binary.clone(),
            raw_telemetry_path: vm.telemetry_path.clone(),
            raw_telemetry_transport: vm.telemetry_transport,
            raw_telemetry_service_name: vm.telemetry_service_name.clone(),
            raw_telemetry_max_age: Duration::from_secs(vm.telemetry_max_age_seconds),
            raw_telemetry_future_tolerance: Duration::from_secs(
                vm.telemetry_future_tolerance_seconds,
            ),
            host_min_headroom_bytes: file.host_min_headroom_bytes,
            compatibility_attestation_path: vm.compatibility_attestation_path.clone(),
            automatic_windows_shrink: vm.automatic_windows_shrink,
            observe_only: file.observe_only,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), HostConfigError> {
        if self.vm_name.trim().is_empty() {
            return Err(HostConfigError::Empty("name"));
        }
        if self.priority == 0 {
            return Err(HostConfigError::InvalidPriority);
        }
        if self.alias.is_empty()
            || !self
                .alias
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        {
            return Err(HostConfigError::InvalidAlias);
        }
        if self.min_memory_bytes == 0 || self.min_memory_bytes > self.max_memory_bytes {
            return Err(HostConfigError::InvalidMemoryRange);
        }
        if self.grow_step_bytes == 0 || self.shrink_step_bytes == 0 {
            return Err(HostConfigError::InvalidResizeStep);
        }
        if self.physical_reserve_bytes == 0
            || self.commit_reserve_bytes == 0
            || self.safe_floor_physical_reserve_bytes == 0
            || self.safe_floor_commit_reserve_bytes == 0
            || self.downward_hysteresis_bytes == 0
        {
            return Err(HostConfigError::InvalidTargetPolicy(
                "reserves and hysteresis must be positive",
            ));
        }
        if self.host_min_headroom_bytes == 0 {
            return Err(HostConfigError::InvalidTargetPolicy(
                "host minimum headroom must be positive",
            ));
        }
        if self.safe_floor_physical_reserve_bytes > self.physical_reserve_bytes
            || self.safe_floor_commit_reserve_bytes > self.commit_reserve_bytes
        {
            return Err(HostConfigError::InvalidReserveOrder);
        }
        if self.poll_interval.is_zero()
            || self.command_timeout.is_zero()
            || self.convergence_timeout.is_zero()
            || self.raw_telemetry_max_age.is_zero()
            || self.raw_telemetry_future_tolerance.is_zero()
            || self.reclaim_history.is_zero()
            || self.reclaim_max_gap.is_zero()
        {
            return Err(HostConfigError::InvalidDuration);
        }
        if self.reclaim_max_gap > self.reclaim_history {
            return Err(HostConfigError::InvalidTargetPolicy(
                "reclaim maximum gap exceeds history window",
            ));
        }
        for (key, value) in [
            ("virsh_binary", &self.virsh_binary),
            (
                "compatibility_attestation_path",
                &self.compatibility_attestation_path,
            ),
            ("telemetry_path", &self.raw_telemetry_path),
            ("telemetry_service_name", &self.raw_telemetry_service_name),
            ("policy_state_path", &self.policy_state_path),
        ] {
            if value.trim().is_empty() {
                return Err(HostConfigError::Empty(key));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    const ONE_VM: &str = r#"
poll_interval_seconds = 10
command_timeout_seconds = 30
convergence_timeout_seconds = 120
host_min_headroom_bytes = 4294967296

[[vm]]
name = "win11"
alias = "ua-virtiomem0"
priority = 100
min_memory_bytes = 1073741824
max_memory_bytes = 17179869184
grow_step_bytes = 1073741824
shrink_step_bytes = 67108864
fixed_visible_base_bytes = 8589934592
physical_reserve_bytes = 2147483648
commit_reserve_bytes = 2147483648
safe_floor_physical_reserve_bytes = 1073741824
safe_floor_commit_reserve_bytes = 1073741824
reclaim_history_seconds = 600
downward_hysteresis_bytes = 268435456
policy_state_path = "/var/lib/virtio-mem-host/state/win11-policy.json"
compatibility_attestation_path = "/etc/virtio-mem-host/win11.attestation.json"
telemetry_transport = "qga-file"
telemetry_path = 'C:\ProgramData\VirtioMem\telemetry.jsonl'
telemetry_service_name = "VirtioMemService"
telemetry_max_age_seconds = 60
telemetry_future_tolerance_seconds = 5
"#;

    fn second_vm(name: &str, priority: u32, state: &str) -> String {
        ONE_VM
            .split_once("[[vm]]")
            .expect("fixture vm table")
            .1
            .replace("\"win11\"", &format!("\"{name}\""))
            .replace("priority = 100", &format!("priority = {priority}"))
            .replace("/var/lib/virtio-mem-host/state/win11-policy.json", state)
    }

    fn parse_one(text: &str) -> Result<HostConfig, HostConfigError> {
        HostConfig::parse(text).map(|mut vms| vms.remove(0))
    }

    fn vm_error(source: HostConfigError) -> HostConfigError {
        HostConfigError::Vm {
            vm: "win11".to_owned(),
            source: Box::new(source),
        }
    }

    #[test]
    fn resolves_global_and_vm_settings_with_safe_defaults() {
        let config = parse_one(ONE_VM).expect("valid config");
        assert_eq!(config.vm_name, "win11");
        assert_eq!(config.priority, 100);
        assert_eq!(config.max_memory_bytes, 16 * GIB);
        assert_eq!(config.poll_interval, Duration::from_secs(10));
        assert_eq!(config.host_min_headroom_bytes, 4 * GIB);
        assert_eq!(
            config.raw_telemetry_transport,
            RawTelemetryTransport::QgaFile
        );
        assert_eq!(
            config.raw_telemetry_path,
            r"C:\ProgramData\VirtioMem\telemetry.jsonl"
        );
        assert_eq!(config.virsh_binary, "virsh");
        assert_eq!(config.reclaim_max_gap, Duration::from_secs(20));
        assert!(!config.automatic_windows_shrink);
        assert!(!config.observe_only);
    }

    #[test]
    fn unknown_and_retired_keys_fail_instead_of_being_ignored() {
        // A retired `shadow` mode silently ignored would turn a configured
        // non-actuating run into one that resizes.
        for retired in [
            "pressure_policy_mode = \"shadow\"\n",
            "demand_source = \"guest-stats\"\n",
            "shrink_renotification = true\n",
        ] {
            let error = HostConfig::parse(&format!("{retired}{ONE_VM}"))
                .expect_err("unknown top-level key");
            assert!(matches!(error, HostConfigError::Parse(_)), "{error}");
        }
        let error =
            HostConfig::parse(&ONE_VM.replace("priority = 100", "priority = 100\nweight = 3"))
                .expect_err("unknown vm key");
        assert!(matches!(error, HostConfigError::Parse(_)), "{error}");
    }

    #[test]
    fn raw_telemetry_transport_is_explicit_and_closed() {
        let file = parse_one(&ONE_VM.replace("\"qga-file\"", "\"file\"")).expect("file");
        assert_eq!(file.raw_telemetry_transport, RawTelemetryTransport::File);
        assert!(matches!(
            HostConfig::parse(&ONE_VM.replace("\"qga-file\"", "\"ssh\"")),
            Err(HostConfigError::Parse(_))
        ));
    }

    #[test]
    fn automatic_shrink_and_observe_only_require_explicit_opt_in() {
        let config = parse_one(&format!(
            "observe_only = true\n{}",
            ONE_VM.replace(
                "priority = 100",
                "priority = 100\nautomatic_windows_shrink = true"
            )
        ))
        .expect("opted-in config");
        assert!(config.automatic_windows_shrink);
        assert!(config.observe_only);
    }

    #[test]
    fn several_vms_carry_their_priorities_but_the_controller_serves_one() {
        let text = format!(
            "{ONE_VM}\n[[vm]]{}",
            second_vm(
                "win10_test",
                80,
                "/var/lib/virtio-mem-host/state/win10-policy.json"
            )
        );
        let vms = HostConfig::parse(&text).expect("two-member pool");
        assert_eq!(
            vms.iter()
                .map(|vm| (vm.vm_name.as_str(), vm.priority))
                .collect::<Vec<_>>(),
            [("win11", 100), ("win10_test", 80)]
        );

        let directory =
            std::env::temp_dir().join(format!("virtio-mem-host-config-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("fixture directory");
        let path = directory.join("pool.toml");
        std::fs::write(&path, &text).expect("write fixture");
        assert_eq!(
            HostConfig::load_single(&path),
            Err(HostConfigError::PoolNotWired(2))
        );
        std::fs::write(&path, ONE_VM).expect("write fixture");
        assert_eq!(
            HostConfig::load_single(&path).map(|vm| vm.vm_name),
            Ok("win11".to_owned())
        );
        std::fs::remove_dir_all(&directory).expect("remove fixture");
    }

    #[test]
    fn vms_must_not_share_a_name_or_policy_state() {
        let duplicate_name = format!(
            "{ONE_VM}\n[[vm]]{}",
            second_vm("win11", 80, "/var/lib/virtio-mem-host/state/other.json")
        );
        assert_eq!(
            HostConfig::parse(&duplicate_name),
            Err(HostConfigError::DuplicateVm("win11".to_owned()))
        );
        let shared_state = format!(
            "{ONE_VM}\n[[vm]]{}",
            second_vm(
                "win10_test",
                80,
                "/var/lib/virtio-mem-host/state/win11-policy.json"
            )
        );
        assert_eq!(
            HostConfig::parse(&shared_state),
            Err(HostConfigError::SharedPolicyState(
                "/var/lib/virtio-mem-host/state/win11-policy.json".to_owned()
            ))
        );
        assert_eq!(
            HostConfig::parse("poll_interval_seconds = 1\ncommand_timeout_seconds = 1\nconvergence_timeout_seconds = 1\nhost_min_headroom_bytes = 1\n"),
            Err(HostConfigError::NoVm)
        );
    }

    #[test]
    fn invalid_vm_settings_name_the_vm() {
        for (from, to, expected) in [
            (
                "priority = 100",
                "priority = 0",
                HostConfigError::InvalidPriority,
            ),
            (
                "\"ua-virtiomem0\"",
                "\"bad;alias\"",
                HostConfigError::InvalidAlias,
            ),
            (
                "min_memory_bytes = 1073741824",
                "min_memory_bytes = 34359738368",
                HostConfigError::InvalidMemoryRange,
            ),
            (
                "safe_floor_commit_reserve_bytes = 1073741824",
                "safe_floor_commit_reserve_bytes = 4294967296",
                HostConfigError::InvalidReserveOrder,
            ),
            (
                "telemetry_max_age_seconds = 60",
                "telemetry_max_age_seconds = 0",
                HostConfigError::InvalidDuration,
            ),
            (
                "\"/etc/virtio-mem-host/win11.attestation.json\"",
                "\" \"",
                HostConfigError::Empty("compatibility_attestation_path"),
            ),
            (
                r"'C:\ProgramData\VirtioMem\telemetry.jsonl'",
                "''",
                HostConfigError::Empty("telemetry_path"),
            ),
        ] {
            assert_eq!(
                HostConfig::parse(&ONE_VM.replace(from, to)),
                Err(vm_error(expected)),
                "{to}"
            );
        }
    }

    #[test]
    fn shipped_example_is_refused_until_its_markers_are_replaced() {
        let example = include_str!("../systemd/virtio-mem-host.toml.example");
        assert!(matches!(
            HostConfig::parse(example),
            Err(HostConfigError::Parse(_))
        ));
        // Replacing only the numeric markers proves the example's keys are the
        // loader's keys; string markers stay as harmless placeholders.
        let filled = example
            .lines()
            .map(|line| match line.split_once(" = \"REQUIRED_") {
                Some((key, _)) if key.ends_with("_bytes") || key == "priority" => {
                    format!("{key} = {GIB}")
                }
                Some(("reclaim_history_seconds", _)) => "reclaim_history_seconds = 600".to_owned(),
                Some((key, _)) if key.ends_with("_seconds") => format!("{key} = 60"),
                _ => line.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let config = parse_one(&filled).expect("filled example");
        assert_eq!(config.vm_name, "REQUIRED_VM_NAME");
        assert!(!config.automatic_windows_shrink && !config.observe_only);
    }
}
