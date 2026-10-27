use std::io::Read;
use std::time::Duration;

use virtio_mem_core::{
    GuestMemoryPolicy, WindowsServiceConfigFile, WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION,
};

use crate::error::ConfigurationError;

pub const DEFAULT_CONFIG_PATH: &str = r"C:\ProgramData\VirtioMemService\config.json";

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
    pub policy: GuestMemoryPolicy,
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
            if value.chars().any(char::is_control) {
                return Err(ConfigurationError::InvalidFile(format!(
                    "{field} contains a control character"
                )));
            }
        }

        if self.poll_interval.is_zero() {
            return Err(ConfigurationError::InvalidPollInterval);
        }
        if self.shutdown_timeout.is_zero() {
            return Err(ConfigurationError::InvalidShutdownTimeout);
        }

        for duration in [self.poll_interval, self.shutdown_timeout] {
            if duration.subsec_nanos() % 1_000_000 != 0
                || u64::try_from(duration.as_millis()).is_err()
            {
                return Err(ConfigurationError::DurationOverflow);
            }
        }
        let wire = WindowsServiceConfigFile::try_from(self)?;
        wire.validate()
            .and_then(|()| wire.validate_paths(&self.config_path))
            .map_err(ConfigurationError::InvalidFile)?;
        #[cfg(windows)]
        validate_file_identity(&self.config_path, &self.demand_report_path)?;
        Ok(())
    }

    pub fn load_default() -> Result<Self, ConfigurationError> {
        Self::load_from_path(DEFAULT_CONFIG_PATH)
    }

    pub fn load_from_path(path: impl AsRef<std::path::Path>) -> Result<Self, ConfigurationError> {
        let path = path.as_ref();
        const MAX_CONFIG_BYTES: u64 = 64 * 1024;
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigurationError::MissingFile(
                    path.to_string_lossy().into_owned(),
                ))
            }
            Err(error) => return Err(ConfigurationError::FileIo(error.to_string())),
        };
        let mut contents = String::new();
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut contents)
            .map_err(|error| ConfigurationError::FileIo(error.to_string()))?;
        if contents.len() as u64 > MAX_CONFIG_BYTES {
            return Err(ConfigurationError::InvalidFile(
                "configuration exceeds 64 KiB".to_owned(),
            ));
        }
        let persisted: WindowsServiceConfigFile = serde_json::from_str(&contents)
            .map_err(|error| ConfigurationError::InvalidFile(error.to_string()))?;
        if persisted.schema_version != WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION {
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
            policy: persisted.policy,
        };
        config.validate()?;
        Ok(config)
    }
}

impl TryFrom<&ServiceConfig> for WindowsServiceConfigFile {
    type Error = ConfigurationError;

    fn try_from(config: &ServiceConfig) -> Result<Self, Self::Error> {
        let poll_interval_millis = u64::try_from(config.poll_interval.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;
        let shutdown_timeout_millis = u64::try_from(config.shutdown_timeout.as_millis())
            .map_err(|_| ConfigurationError::DurationOverflow)?;
        Ok(Self {
            schema_version: WINDOWS_SERVICE_CONFIG_SCHEMA_VERSION,
            vm_name: config.vm_name.clone(),
            service_name: config.service_name.clone(),
            display_name: config.display_name.clone(),
            description: config.description.clone(),
            demand_report_path: config.demand_report_path.clone(),
            service_account: config.service_account.clone(),
            poll_interval_millis,
            shutdown_timeout_millis,
            policy: config.policy,
        })
    }
}

#[cfg(windows)]
pub(crate) fn validate_file_identity(config: &str, report: &str) -> Result<(), ConfigurationError> {
    use std::os::windows::io::AsRawHandle;
    use winapi::um::fileapi::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let identify = |path: &str| -> Result<Option<(u32, u32, u32)>, ConfigurationError> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ConfigurationError::FileIo(error.to_string())),
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0 {
            return Err(ConfigurationError::FileIo(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        Ok(Some((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        )))
    };
    if let Some(config_id) = identify(config)? {
        for path in [
            report.to_owned(),
            virtio_mem_core::host_status_path(report),
            virtio_mem_core::request_state_path(report),
        ]
        .into_iter()
        .chain((1..=3).map(|index| format!("{report}.{index}")))
        {
            if identify(&path)? == Some(config_id) {
                return Err(ConfigurationError::InvalidFile(
                    "configuration shares a file identity with a publication file".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

/// A valid configuration shared by this crate's tests.
#[cfg(test)]
pub(crate) fn test_config() -> ServiceConfig {
    const MIB: u64 = 1024 * 1024;
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
        policy: GuestMemoryPolicy {
            target: virtio_mem_core::TargetPolicyConfig {
                configured_minimum_bytes: 0,
                physical_reserve_bytes: 2048 * MIB,
                commit_reserve_bytes: 2048 * MIB,
                safe_floor_physical_reserve_bytes: 1024 * MIB,
                safe_floor_commit_reserve_bytes: 1024 * MIB,
                history_window_millis: 600_000,
                maximum_gap_millis: 60_000,
                downward_hysteresis_bytes: 256 * MIB,
            },
            grow_step_bytes: 1024 * MIB,
            shrink_step_bytes: 64 * MIB,
            host_status_max_age_millis: 60_000,
            request_lifetime_millis: 30_000,
            max_removal_pause_windows: 16,
        },
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
    fn rejects_native_string_path_collisions_and_lossy_durations() {
        let mut config = test_config();
        config.config_path = r"c:\TEST\.\TELEMETRY.jsonl".to_owned();
        assert!(config.validate().is_err());
        config.config_path = format!("{}.1", config.demand_report_path);
        assert!(config.validate().is_err());
        config.config_path = virtio_mem_core::request_state_path(&config.demand_report_path);
        assert!(config.validate().is_err());
        config = test_config();
        config.description.push('\0');
        assert!(config.validate().is_err());
        config = test_config();
        config.poll_interval = Duration::from_nanos(1);
        assert_eq!(config.validate(), Err(ConfigurationError::DurationOverflow));
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

        std::fs::write(
            &path,
            serde_json::to_vec(&WindowsServiceConfigFile::try_from(&expected).unwrap()).unwrap(),
        )
        .unwrap();
        let loaded = ServiceConfig::load_from_path(&path).expect("configuration should load");

        assert_eq!(loaded, expected);
        std::fs::remove_file(path).expect("test configuration should be removed");
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let path = test_path("config-schema");
        let fixture = WindowsServiceConfigFile {
            schema_version: 99,
            vm_name: "test-vm".to_owned(),
            service_name: "TestService".to_owned(),
            display_name: "Test service".to_owned(),
            description: "Test configuration".to_owned(),
            demand_report_path: r"C:\test\telemetry.jsonl".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            poll_interval_millis: 30_000,
            shutdown_timeout_millis: 30_000,
            policy: test_config().policy,
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
