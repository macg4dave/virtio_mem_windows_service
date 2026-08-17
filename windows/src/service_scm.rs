use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicU8, Ordering};

use winapi::shared::minwindef::{DWORD, FALSE};
use winapi::um::winsvc::{
    ControlService, CreateServiceW, OpenSCManagerW, OpenServiceW, SERVICE_ACCEPT_SHUTDOWN,
    SERVICE_ACCEPT_STOP, SERVICE_CONTROL_SHUTDOWN, SERVICE_CONTROL_STOP, SERVICE_RUNNING,
    SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SC_MANAGER_CREATE_SERVICE,
};

const SERVICE_ALL_ACCESS: DWORD = 0xF01FF;
const SERVICE_AUTO_START: DWORD = 0x00000002;
const SERVICE_ERROR_NORMAL: DWORD = 0x00000000;
const SERVICE_WIN32_OWN_PROCESS: DWORD = 0x00000010;

use crate::config::ServiceConfig;
use crate::service_host::{ServiceState, StopSignal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScmServiceState {
    StartPending,
    Running,
    StopPending,
    Stopped,
    Failed,
}

impl From<ServiceState> for ScmServiceState {
    fn from(state: ServiceState) -> Self {
        match state {
            ServiceState::Created => Self::StartPending,
            ServiceState::StartPending => Self::StartPending,
            ServiceState::Running => Self::Running,
            ServiceState::StopPending => Self::StopPending,
            ServiceState::Stopped => Self::Stopped,
            ServiceState::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScmServiceStatus {
    pub current_state: DWORD,
    pub controls_accepted: DWORD,
    pub exit_code: DWORD,
}

impl ScmServiceStatus {
    pub fn from_state(state: ServiceState) -> Self {
        let current_state = match ScmServiceState::from(state) {
            ScmServiceState::StartPending => SERVICE_START_PENDING,
            ScmServiceState::Running => SERVICE_RUNNING,
            ScmServiceState::StopPending => SERVICE_STOP_PENDING,
            ScmServiceState::Stopped => SERVICE_STOPPED,
            ScmServiceState::Failed => SERVICE_STOPPED,
        } as DWORD;

        let controls_accepted = match ScmServiceState::from(state) {
            ScmServiceState::Running => SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            ScmServiceState::StopPending => SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
            ScmServiceState::StartPending => 0,
            ScmServiceState::Stopped | ScmServiceState::Failed => 0,
        };

        let exit_code = match ScmServiceState::from(state) {
            ScmServiceState::Failed => 1,
            _ => 0,
        };

        Self {
            current_state,
            controls_accepted,
            exit_code,
        }
    }
}

pub struct ScmHandler {
    stop: StopSignal,
    state: AtomicU8,
}

impl ScmHandler {
    pub fn new(stop: StopSignal) -> Self {
        Self {
            stop,
            state: AtomicU8::new(ServiceState::Created as u8),
        }
    }

    pub fn state(&self) -> ServiceState {
        ServiceState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn transition_to(&self, new_state: ServiceState) {
        self.state.store(new_state as u8, Ordering::Release);
    }

    pub fn accept_control(&self, control: DWORD) -> bool {
        if control == SERVICE_CONTROL_STOP || control == SERVICE_CONTROL_SHUTDOWN {
            self.stop.cancel();
            self.state.store(ServiceState::StopPending as u8, Ordering::Release);
            return true;
        }

        false
    }

    pub fn status(&self) -> ScmServiceStatus {
        ScmServiceStatus::from_state(self.state())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServiceRegistration {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub executable_path: String,
    pub service_account: String,
    pub startup_type: DWORD,
}

impl WindowsServiceRegistration {
    pub fn from_config(config: &ServiceConfig) -> Result<Self, String> {
        config.validate().map_err(|error| error.to_string())?;

        let executable_path = std::env::current_exe()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("resolve executable path: {error}"))?;

        Ok(Self {
            service_name: config.service_name.clone(),
            display_name: config.display_name.clone(),
            description: config.description.clone(),
            executable_path,
            service_account: config.service_account.clone(),
            startup_type: SERVICE_AUTO_START,
        })
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(once(0))
        .collect()
}

pub fn install_service(config: &ServiceConfig) -> Result<(), String> {
    let registration = WindowsServiceRegistration::from_config(config)?;
    let manager_name = to_wide("");
    let manager = unsafe { OpenSCManagerW(manager_name.as_ptr(), std::ptr::null(), SC_MANAGER_CREATE_SERVICE) };
    if manager.is_null() {
        return Err("OpenSCManagerW failed to open the service manager".to_owned());
    }

    let service_name = to_wide(&registration.service_name);
    let display_name = to_wide(&registration.display_name);
    let executable = to_wide(&registration.executable_path);
    let account = to_wide(&registration.service_account);
    let password = to_wide("");

    let service = unsafe {
        CreateServiceW(
            manager,
            service_name.as_ptr(),
            display_name.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            registration.startup_type,
            SERVICE_ERROR_NORMAL,
            executable.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            account.as_ptr(),
            password.as_ptr(),
        )
    };

    if service.is_null() {
        let os_error = std::io::Error::last_os_error();
        return Err(format!(
            "CreateServiceW failed for {} with error {os_error}",
            registration.service_name,
        ));
    }

    unsafe {
        winapi::um::winsvc::CloseServiceHandle(service);
        winapi::um::winsvc::CloseServiceHandle(manager);
    }

    Ok(())
}

pub fn stop_service(service_name: &str) -> Result<(), String> {
    let manager_name = to_wide("");
    let manager = unsafe { OpenSCManagerW(manager_name.as_ptr(), std::ptr::null(), 0) };
    if manager.is_null() {
        return Err("OpenSCManagerW failed while preparing to stop the service".to_owned());
    }

    let service_name_u16 = to_wide(service_name);
    let service = unsafe { OpenServiceW(manager, service_name_u16.as_ptr(), SERVICE_STOP) };
    if service.is_null() {
        unsafe {
            winapi::um::winsvc::CloseServiceHandle(manager);
        }
        return Err(format!("OpenServiceW failed for {service_name}"));
    }

    let mut status = SERVICE_STATUS {
        dwServiceType: 0,
        dwCurrentState: 0,
        dwControlsAccepted: 0,
        dwWin32ExitCode: 0,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };

    let stopped = unsafe { ControlService(service, SERVICE_CONTROL_STOP, &mut status) };
    unsafe {
        winapi::um::winsvc::CloseServiceHandle(service);
        winapi::um::winsvc::CloseServiceHandle(manager);
    }

    if stopped == FALSE {
        return Err(format!("ControlService failed for {service_name}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use winapi::um::winsvc::{
        SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_RUNNING, SERVICE_START_PENDING,
        SERVICE_STOPPED, SERVICE_STOP_PENDING,
    };

    #[test]
    fn maps_service_state_to_windows_status_codes() {
        assert_eq!(
            ScmServiceStatus::from_state(ServiceState::StartPending).current_state,
            SERVICE_START_PENDING as DWORD
        );
        assert_eq!(
            ScmServiceStatus::from_state(ServiceState::Running).current_state,
            SERVICE_RUNNING as DWORD
        );
        assert_eq!(
            ScmServiceStatus::from_state(ServiceState::StopPending).current_state,
            SERVICE_STOP_PENDING as DWORD
        );
        assert_eq!(
            ScmServiceStatus::from_state(ServiceState::Stopped).current_state,
            SERVICE_STOPPED as DWORD
        );
    }

    #[test]
    fn accepts_stop_and_shutdown_controls_only_while_running() {
        let handler = ScmHandler::new(StopSignal::new());
        handler.transition_to(ServiceState::Running);

        assert_eq!(
            handler.status().controls_accepted,
            (SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN) as DWORD
        );

        assert!(handler.accept_control(SERVICE_CONTROL_STOP));
        assert_eq!(handler.state(), ServiceState::StopPending);
        assert!(handler.stop.is_cancelled());
    }

    #[test]
    fn marks_failed_workers_with_non_zero_exit_code() {
        let status = ScmServiceStatus::from_state(ServiceState::Failed);

        assert_eq!(status.current_state, SERVICE_STOPPED as DWORD);
        assert_eq!(status.exit_code, 1);
    }
}
