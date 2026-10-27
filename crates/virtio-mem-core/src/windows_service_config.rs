//! The Windows telemetry service's configuration file format. It lives here so
//! the service and the deployment tooling share one serialization.

use serde::{Deserialize, Serialize};

pub const WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION: u32 = 4;

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
        if !self.vm_name.is_ascii()
            || self.vm_name.len() > 128
            || !self.service_name.is_ascii()
            || self.service_name.len() > 128
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
        Ok(())
    }

    pub fn validate_paths(&self, config_path: &str) -> Result<(), String> {
        let config = normalize_windows_path(config_path)?;
        let report = normalize_windows_path(&self.demand_report_path)?;
        if config == report
            || (1..=3).any(|index| config == format!("{report}.{index}"))
            || config.starts_with(&format!("{report}.tmp-"))
        {
            return Err(
                "configuration path collides with telemetry publication or retention".to_owned(),
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
}
