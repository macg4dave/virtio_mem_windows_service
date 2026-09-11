use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::ConfigurationError;

pub const DEFAULT_CONFIG_PATH: &str = r"C:\ProgramData\VirtioMemService\config.json";
const CONFIG_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub vm_name: String,
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub demand_report_path: String,
    pub service_account: String,
    pub config_path: String,
    pub poll_interval: Duration,
    pub shutdown_timeout: Duration,
}

impl ServiceConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        for (value, field) in [
            (&self.vm_name, "VM name"),
            (&self.service_name, "service name"),
            (&self.display_name, "display name"),
            (&self.description, "description"),
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
                return Err(ConfigurationError::MissingFile(
                    path.to_string_lossy().into_owned(),
                ))
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
            vm_name: persisted.vm_name,
            service_name: persisted.service_name,
            display_name: persisted.display_name,
            description: persisted.description,
            demand_report_path: persisted.demand_report_path,
            service_account: persisted.service_account,
            config_path: path.to_string_lossy().into_owned(),
            poll_interval: Duration::from_millis(persisted.poll_interval_millis),
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
#[serde(deny_unknown_fields)]
struct PersistedServiceConfig {
    schema_version: u32,
    vm_name: String,
    service_name: String,
    display_name: String,
    description: String,
    demand_report_path: String,
    service_account: String,
    poll_interval_millis: u64,
    shutdown_timeout_millis: u64,
}

impl TryFrom<&ServiceConfig> for PersistedServiceConfig {
    type Error = ConfigurationError;

    fn try_from(config: &ServiceConfig) -> Result<Self, Self::Error> {
        let poll_interval_millis = u64::try_from(config.poll_interval.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;
        let shutdown_timeout_millis = u64::try_from(config.shutdown_timeout.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;
        Ok(Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            vm_name: config.vm_name.clone(),
            service_name: config.service_name.clone(),
            display_name: config.display_name.clone(),
            description: config.description.clone(),
            demand_report_path: config.demand_report_path.clone(),
            service_account: config.service_account.clone(),
            poll_interval_millis,
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

    fn test_config() -> ServiceConfig {
        ServiceConfig {
            vm_name: "test-vm".to_owned(),
            service_name: "TestService".to_owned(),
            display_name: "Test service".to_owned(),
            description: "Test configuration".to_owned(),
            demand_report_path: r"C:\test\telemetry.jsonl".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            config_path: DEFAULT_CONFIG_PATH.to_owned(),
            poll_interval: Duration::from_millis(20),
            shutdown_timeout: Duration::from_millis(30),
        }
    }

    #[test]
    fn explicit_configuration_is_valid() {
        assert!(test_config().validate().is_ok());
    }

    #[test]
    fn rejects_empty_identity_fields() {
        let mut config = test_config();
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
            ..test_config()
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
            ..test_config()
        };
        assert_eq!(
            config.validate(),
            Err(ConfigurationError::InvalidPollInterval)
        );

        config.poll_interval = Duration::from_secs(1);
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
            ..test_config()
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
            vm_name: "test-vm".to_owned(),
            service_name: "TestService".to_owned(),
            display_name: "Test service".to_owned(),
            description: "Test configuration".to_owned(),
            demand_report_path: r"C:\test\telemetry.jsonl".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            poll_interval_millis: 30_000,
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
    fn missing_configuration_is_rejected() {
        let path = test_path("config-missing");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            ServiceConfig::load_from_path(&path),
            Err(ConfigurationError::MissingFile(
                path.to_string_lossy().into_owned()
            ))
        );
    }
}
