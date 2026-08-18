use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::ConfigurationError;

pub const DEFAULT_SERVICE_NAME: &str = "VirtioMemService";
pub const DEFAULT_DISPLAY_NAME: &str = "Virtio-mem Windows Service";
pub const DEFAULT_DESCRIPTION: &str = "Monitors Windows guest memory for virtio-mem coordination";
pub const DEFAULT_QGA_PIPE_PATH: &str = r"\\.\Global\org.qemu.guest_agent.0";
pub const DEFAULT_SERVICE_ACCOUNT: &str = r"NT AUTHORITY\LocalService";
pub const DEFAULT_DEMAND_REPORT_PATH: &str =
    r"C:\ProgramData\VirtioMemService\demand-reports.jsonl";
pub const DEFAULT_CONFIG_PATH: &str = r"C:\ProgramData\VirtioMemService\config.json";
const CONFIG_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub qga_pipe_path: String,
    pub demand_report_path: String,
    pub service_account: String,
    pub config_path: String,
    pub poll_interval: Duration,
    pub qga_operation_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_name: DEFAULT_SERVICE_NAME.to_owned(),
            display_name: DEFAULT_DISPLAY_NAME.to_owned(),
            description: DEFAULT_DESCRIPTION.to_owned(),
            qga_pipe_path: DEFAULT_QGA_PIPE_PATH.to_owned(),
            demand_report_path: DEFAULT_DEMAND_REPORT_PATH.to_owned(),
            service_account: DEFAULT_SERVICE_ACCOUNT.to_owned(),
            config_path: DEFAULT_CONFIG_PATH.to_owned(),
            poll_interval: Duration::from_secs(30),
            qga_operation_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(30),
        }
    }
}

impl ServiceConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        for (value, field) in [
            (&self.service_name, "service name"),
            (&self.display_name, "display name"),
            (&self.description, "description"),
            (&self.qga_pipe_path, "QEMU Guest Agent pipe path"),
            (&self.demand_report_path, "demand report path"),
            (&self.service_account, "service account"),
            (&self.config_path, "configuration path"),
        ] {
            if value.trim().is_empty() {
                return Err(ConfigurationError::EmptyField(field));
            }
        }

        if self.poll_interval.is_zero() {
            return Err(ConfigurationError::InvalidPollInterval);
        }
        if self.qga_operation_timeout.is_zero() {
            return Err(ConfigurationError::InvalidQgaOperationTimeout);
        }
        if self.shutdown_timeout.is_zero() {
            return Err(ConfigurationError::InvalidShutdownTimeout);
        }

        Ok(())
    }

    pub fn load_default() -> Result<Self, ConfigurationError> {
        Self::load_from_path(DEFAULT_CONFIG_PATH)
    }

    pub fn load_from_path(path: impl AsRef<std::path::Path>) -> Result<Self, ConfigurationError> {
        let path = path.as_ref();
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    config_path: path.to_string_lossy().into_owned(),
                    ..Self::default()
                })
            }
            Err(error) => return Err(ConfigurationError::FileIo(error.to_string())),
        };
        let persisted: PersistedServiceConfig = serde_json::from_str(&contents)
            .map_err(|error| ConfigurationError::InvalidFile(error.to_string()))?;
        if persisted.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigurationError::UnsupportedSchemaVersion(
                persisted.schema_version,
            ));
        }
        let config = Self {
            service_name: persisted.service_name,
            display_name: persisted.display_name,
            description: persisted.description,
            qga_pipe_path: persisted.qga_pipe_path,
            demand_report_path: persisted.demand_report_path,
            service_account: persisted.service_account,
            config_path: path.to_string_lossy().into_owned(),
            poll_interval: Duration::from_millis(persisted.poll_interval_millis),
            qga_operation_timeout: Duration::from_millis(persisted.qga_operation_timeout_millis),
            shutdown_timeout: Duration::from_millis(persisted.shutdown_timeout_millis),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), ConfigurationError> {
        self.save_to_path(&self.config_path)
    }

    pub fn save_to_path(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), ConfigurationError> {
        self.validate()?;
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| ConfigurationError::FileIo(error.to_string()))?;
        }
        let persisted = PersistedServiceConfig::try_from(self)?;
        let contents = serde_json::to_string_pretty(&persisted)
            .map_err(|error| ConfigurationError::InvalidFile(error.to_string()))?;
        std::fs::write(path, format!("{contents}\n"))
            .map_err(|error| ConfigurationError::FileIo(error.to_string()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedServiceConfig {
    schema_version: u32,
    service_name: String,
    display_name: String,
    description: String,
    qga_pipe_path: String,
    demand_report_path: String,
    service_account: String,
    poll_interval_millis: u64,
    qga_operation_timeout_millis: u64,
    shutdown_timeout_millis: u64,
}

impl TryFrom<&ServiceConfig> for PersistedServiceConfig {
    type Error = ConfigurationError;

    fn try_from(config: &ServiceConfig) -> Result<Self, Self::Error> {
        let poll_interval_millis = u64::try_from(config.poll_interval.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;
        let shutdown_timeout_millis = u64::try_from(config.shutdown_timeout.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;
        let qga_operation_timeout_millis = u64::try_from(config.qga_operation_timeout.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;

        Ok(Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            service_name: config.service_name.clone(),
            display_name: config.display_name.clone(),
            description: config.description.clone(),
            qga_pipe_path: config.qga_pipe_path.clone(),
            demand_report_path: config.demand_report_path.clone(),
            service_account: config.service_account.clone(),
            poll_interval_millis,
            qga_operation_timeout_millis,
            shutdown_timeout_millis,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "virtio-mem-service-{name}-{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn default_configuration_is_valid() {
        assert!(ServiceConfig::default().validate().is_ok());
    }

    #[test]
    fn rejects_empty_identity_fields() {
        let mut config = ServiceConfig::default();
        config.service_name.clear();

        assert_eq!(
            config.validate(),
            Err(ConfigurationError::EmptyField("service name"))
        );
    }

    #[test]
    fn rejects_empty_demand_report_path() {
        let config = ServiceConfig {
            demand_report_path: String::new(),
            ..ServiceConfig::default()
        };

        assert_eq!(
            config.validate(),
            Err(ConfigurationError::EmptyField("demand report path"))
        );
    }

    #[test]
    fn rejects_zero_timeouts() {
        let mut config = ServiceConfig {
            poll_interval: Duration::ZERO,
            ..ServiceConfig::default()
        };
        assert_eq!(
            config.validate(),
            Err(ConfigurationError::InvalidPollInterval)
        );

        config.poll_interval = Duration::from_secs(1);
        config.qga_operation_timeout = Duration::ZERO;
        assert_eq!(
            config.validate(),
            Err(ConfigurationError::InvalidQgaOperationTimeout)
        );

        config.qga_operation_timeout = Duration::from_secs(5);
        config.shutdown_timeout = Duration::ZERO;
        assert_eq!(
            config.validate(),
            Err(ConfigurationError::InvalidShutdownTimeout)
        );
    }

    #[test]
    fn persists_and_loads_versioned_configuration() {
        let path = test_path("config-round-trip");
        let _ = std::fs::remove_file(&path);
        let expected = ServiceConfig {
            config_path: path.to_string_lossy().into_owned(),
            poll_interval: Duration::from_millis(1250),
            shutdown_timeout: Duration::from_millis(2750),
            ..ServiceConfig::default()
        };

        expected.save().expect("configuration should save");
        let loaded = ServiceConfig::load_from_path(&path).expect("configuration should load");

        assert_eq!(loaded, expected);
        std::fs::remove_file(path).expect("test configuration should be removed");
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let path = test_path("config-schema");
        let fixture = PersistedServiceConfig {
            schema_version: 99,
            service_name: DEFAULT_SERVICE_NAME.to_owned(),
            display_name: DEFAULT_DISPLAY_NAME.to_owned(),
            description: DEFAULT_DESCRIPTION.to_owned(),
            qga_pipe_path: DEFAULT_QGA_PIPE_PATH.to_owned(),
            demand_report_path: DEFAULT_DEMAND_REPORT_PATH.to_owned(),
            service_account: DEFAULT_SERVICE_ACCOUNT.to_owned(),
            poll_interval_millis: 30_000,
            qga_operation_timeout_millis: 5_000,
            shutdown_timeout_millis: 30_000,
        };
        std::fs::write(
            &path,
            serde_json::to_string(&fixture).expect("fixture should encode"),
        )
        .expect("fixture should write");

        assert_eq!(
            ServiceConfig::load_from_path(&path),
            Err(ConfigurationError::UnsupportedSchemaVersion(99))
        );
        std::fs::remove_file(path).expect("test configuration should be removed");
    }

    #[test]
    fn missing_configuration_uses_validated_defaults() {
        let path = test_path("config-missing");
        let _ = std::fs::remove_file(&path);

        let config = ServiceConfig::load_from_path(&path).expect("missing config is allowed");

        assert_eq!(config.config_path, path.to_string_lossy());
        assert!(config.validate().is_ok());
    }
}
