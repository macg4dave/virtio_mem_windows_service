use std::time::Duration;

use crate::error::ConfigurationError;

pub const DEFAULT_SERVICE_NAME: &str = "VirtioMemService";
pub const DEFAULT_DISPLAY_NAME: &str = "Virtio-mem Windows Service";
pub const DEFAULT_DESCRIPTION: &str = "Monitors Windows guest memory for virtio-mem coordination";
pub const DEFAULT_QGA_PIPE_PATH: &str = r"\\.\Global\org.qemu.guest_agent.0";
pub const DEFAULT_SERVICE_ACCOUNT: &str = r"NT AUTHORITY\LocalService";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub qga_pipe_path: String,
    pub service_account: String,
    pub poll_interval: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_name: DEFAULT_SERVICE_NAME.to_owned(),
            display_name: DEFAULT_DISPLAY_NAME.to_owned(),
            description: DEFAULT_DESCRIPTION.to_owned(),
            qga_pipe_path: DEFAULT_QGA_PIPE_PATH.to_owned(),
            service_account: DEFAULT_SERVICE_ACCOUNT.to_owned(),
            poll_interval: Duration::from_secs(30),
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
            (&self.service_account, "service account"),
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        config.shutdown_timeout = Duration::ZERO;
        assert_eq!(
            config.validate(),
            Err(ConfigurationError::InvalidShutdownTimeout)
        );
    }
}
