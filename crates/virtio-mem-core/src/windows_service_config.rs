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
