//! The Windows memory service's configuration file format. It lives here so
//! the service and the deployment tooling share one serialization.

use serde::{Deserialize, Serialize};

use crate::GuestMemoryPolicy;

pub const WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsServiceConfigFile {
    pub schema_version: u32,
    pub vm_name: String,
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub demand_report_path: String,
    pub service_account: String,
    pub poll_interval_millis: u64,
    pub shutdown_timeout_millis: u64,
    pub policy: GuestMemoryPolicy,
    pub min_ram: u64,
    pub max_ram: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry_path: Option<String>,
}

impl WindowsServiceConfigFile {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION {
            return Err(format!(
                "unsupported configuration schema version: {}",
                self.schema_version
            ));
        }
        for value in [
            &self.vm_name,
            &self.service_name,
            &self.display_name,
            &self.description,
            &self.service_account,
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(
                    "configuration strings must be nonempty and contain no control characters"
                        .to_owned(),
                );
            }
        }
        if !crate::memory_request::valid_identity(&self.vm_name)
            || !crate::memory_request::valid_identity(&self.service_name)
            || self.service_name.contains(['/', '\\'])
        {
            return Err("VM/service identity must be ASCII of at most 128 bytes; service names cannot contain slashes".to_owned());
        }
        normalize_windows_path(&self.demand_report_path)?;
        for millis in [self.poll_interval_millis, self.shutdown_timeout_millis] {
            if millis == 0 || millis >= u64::from(u32::MAX) {
                return Err("configuration timing is zero or too large".to_owned());
            }
        }
        crate::settings::validate_ram_limits(self.min_ram, self.max_ram)?;
        self.policy.validate()
    }

    /// The publication path also names its handoff files, host status and
    /// durable request state as `<path>.<suffix>`; the configuration must be
    /// none of them.
    pub fn validate_paths(&self, config_path: &str) -> Result<(), String> {
        let config = normalize_windows_path(config_path)?;
        if let Some(path) = &self.telemetry_path {
            let telemetry = normalize_windows_path(path)?;
            let control = normalize_windows_path(&self.demand_report_path)?;
            if telemetry == config
                || telemetry == control
                || telemetry.starts_with(&format!("{control}."))
            {
                return Err(
                    "optional telemetry collides with configuration or control files".to_owned(),
                );
            }
        }
        let report = normalize_windows_path(&self.demand_report_path)?;
        if config == report || config.starts_with(&format!("{report}.")) {
            return Err(
                "configuration path collides with the telemetry publication or its derived files"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

/// Compare Windows paths consistently even when deployment runs on Linux.
/// Reject ambiguous Win32 forms rather than guessing their native expansion.
pub fn normalize_windows_path(value: &str) -> Result<String, String> {
    let path = value.replace('/', "\\");
    let bytes = path.as_bytes();
    if value.chars().any(char::is_control)
        || bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
    {
        return Err(
            "path must be an absolute Windows drive path without control characters".to_owned(),
        );
    }
    let mut parts = Vec::new();
    for part in path[3..].split('\\') {
        if part == "." || part.is_empty() {
            continue;
        }
        if part == ".."
            || part.ends_with(['.', ' '])
            || part.contains([':', '"', '<', '>', '|', '?', '*'])
            || matches!(
                part.split('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
                    .as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
        {
            return Err("path contains an ambiguous or reserved Windows component".to_owned());
        }
        parts.push(part.to_lowercase());
    }
    if parts.is_empty() {
        return Err("path must name a file".to_owned());
    }
    Ok(format!(
        "{}:\\{}",
        char::from(bytes[0]).to_ascii_lowercase(),
        parts.join("\\")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_aliases_are_compared_on_every_platform() {
        assert_eq!(
            normalize_windows_path(r"C:\Data\.\CONFIG.json").unwrap(),
            normalize_windows_path("c:/data/config.json").unwrap()
        );
        for path in [
            "relative.json",
            r"C:\data\..\config.json",
            "C:\\data\\name. ",
            "C:\\data\\nul.json",
            "C:\\data\\file:stream",
            "C:\\data\\bad\0.json",
        ] {
            assert!(normalize_windows_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn configuration_cannot_alias_any_derived_publication_file() {
        let config = WindowsServiceConfigFile {
            min_ram: 1 << 30,
            max_ram: 64 << 30,
            telemetry_path: None,
            schema_version: WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION,
            vm_name: "guest".to_owned(),
            service_name: "VirtioMemService".to_owned(),
            display_name: "Virtio memory".to_owned(),
            description: "Virtio memory".to_owned(),
            demand_report_path: r"C:\Data\telemetry.json".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            poll_interval_millis: 1000,
            shutdown_timeout_millis: 1000,
            policy: crate::GuestMemoryPolicy {
                target: crate::TargetPolicyConfig {
                    configured_minimum_bytes: 0,
                    physical_reserve_bytes: 1,
                    commit_reserve_bytes: 1,
                    safe_floor_physical_reserve_bytes: 1,
                    safe_floor_commit_reserve_bytes: 1,
                    history_window_millis: 1,
                    maximum_gap_millis: 1,
                    downward_hysteresis_bytes: 1,
                },
                grow_step_bytes: 1,
                shrink_step_bytes: 1,
                host_status_max_age_millis: 1,
                request_lifetime_millis: 1,
            },
        };
        config.validate().unwrap();
        config.validate_paths(r"C:\Data\config.json").unwrap();
        for path in [
            crate::host_status_path(r"c:/data/TELEMETRY.json"),
            crate::request_state_path(r"C:\Data\telemetry.json"),
            r"C:\Data\telemetry.json.tmp-12".to_owned(),
            r"C:\Data\telemetry.json".to_owned(),
        ] {
            assert!(config.validate_paths(&path).is_err(), "{path}");
        }
    }
}
