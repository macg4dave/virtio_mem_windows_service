use std::path::Path;
use std::time::Duration;

use thiserror::Error;
use virtio_mem_core::settings::{
    self, GlobalSettings, HostVm, VmSettings, CONTROL_PATH, SERVICE_NAME,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostConfigError {
    #[error("invalid host configuration: {0}")]
    Parse(String),
    #[error("host config lists {0} VMs; select one with --vm NAME")]
    VmNotSelected(usize),
    #[error("vm {0} is not in the host config")]
    UnknownVm(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawTelemetryTransport {
    File,
    QgaFile,
}

/// One resolved runtime config from the global, host-list and per-VM files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    pub vm_name: String,
    pub alias: String,
    /// Pool priority; larger is served first when VMs contend.
    pub priority: u32,
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
    /// Evaluate guest requests and log grants, but refuse every request
    /// before any device command.
    pub observe_only: bool,
    pub min_ram: u64,
    pub max_ram: u64,
    pub telemetry_path: Option<String>,
}

/// Every resolved VM plus the server pool budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSettings {
    /// Device bytes shared by every VM. Required with more than one VM.
    pub pool_bytes: Option<u64>,
    pub vms: Vec<HostConfig>,
}

impl HostSettings {
    /// A host list and its sibling global/per-VM files form one configuration.
    pub fn load(path: &Path) -> Result<Self, HostConfigError> {
        let (globals, list, settings) =
            settings::load_configuration(path).map_err(HostConfigError::Parse)?;
        let vms = list
            .vms
            .iter()
            .zip(&settings)
            .map(|(entry, vm)| HostConfig::resolve(&globals, entry, vm))
            .collect();
        Ok(Self {
            pool_bytes: globals.pool_bytes,
            vms,
        })
    }
}

impl HostConfig {
    pub fn load_vm(path: &Path, vm: Option<&str>) -> Result<Self, HostConfigError> {
        let mut vms = HostSettings::load(path)?.vms;
        match vm {
            Some(name) => vms
                .into_iter()
                .find(|config| config.vm_name == name)
                .ok_or_else(|| HostConfigError::UnknownVm(name.to_owned())),
            None if vms.len() == 1 => Ok(vms.remove(0)),
            None => Err(HostConfigError::VmNotSelected(vms.len())),
        }
    }

    fn resolve(globals: &GlobalSettings, entry: &HostVm, vm: &VmSettings) -> Self {
        let name = &entry.vm_name;
        Self {
            vm_name: name.clone(),
            alias: entry.alias.clone(),
            priority: entry.priority,
            policy_state_path: format!("/var/lib/virtio-mem-host/state/{name}-requests.json"),
            compatibility_attestation_path: format!("/etc/virtio-mem-host/{name}.attestation.json"),
            poll_interval: Duration::from_secs(globals.poll_interval_seconds),
            command_timeout: Duration::from_secs(globals.command_timeout_seconds),
            convergence_timeout: Duration::from_secs(globals.convergence_timeout_seconds),
            virsh_binary: "/usr/bin/virsh".to_owned(),
            raw_telemetry_path: CONTROL_PATH.to_owned(),
            raw_telemetry_transport: RawTelemetryTransport::QgaFile,
            raw_telemetry_service_name: SERVICE_NAME.to_owned(),
            raw_telemetry_max_age: Duration::from_secs(globals.poll_interval_seconds * 6),
            raw_telemetry_future_tolerance: Duration::from_secs(globals.poll_interval_seconds),
            host_min_headroom_bytes: globals.host_min_headroom_bytes,
            observe_only: globals.observe_only,
            min_ram: vm.min_ram,
            max_ram: vm.max_ram,
            telemetry_path: entry.telemetry_path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracked_configuration_loads_without_optional_telemetry() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../deploy/config_files/host_list.toml");
        let settings = HostSettings::load(&path).expect("tracked config");
        assert_eq!(settings.vms.len(), 1);
        assert!(settings.vms[0].telemetry_path.is_none());
        assert_eq!(settings.vms[0].raw_telemetry_path, CONTROL_PATH);
        assert_eq!(
            settings.vms[0].policy_state_path,
            "/var/lib/virtio-mem-host/state/win11_gpu-requests.json"
        );
        assert!(HostConfig::load_vm(&path, Some("absent")).is_err());
    }
}
