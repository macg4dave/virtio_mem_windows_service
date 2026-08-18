use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicU8, Ordering};

use winapi::ctypes::c_void;
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
use winapi::um::winsvc::{
    ChangeServiceConfig2W, ControlService, CreateServiceW, DeleteService, OpenSCManagerW,
    OpenServiceW, RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    StartServiceW, SC_ACTION, SC_ACTION_RESTART, SC_MANAGER_CREATE_SERVICE,
    SERVICE_ACCEPT_SHUTDOWN, SERVICE_ACCEPT_STOP, SERVICE_CONFIG_DESCRIPTION,
    SERVICE_CONFIG_FAILURE_ACTIONS, SERVICE_CONFIG_FAILURE_ACTIONS_FLAG, SERVICE_CONTROL_SHUTDOWN,
    SERVICE_CONTROL_STOP, SERVICE_DESCRIPTIONW, SERVICE_FAILURE_ACTIONSW,
    SERVICE_FAILURE_ACTIONS_FLAG, SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS,
    SERVICE_STATUS_HANDLE, SERVICE_STOP, SERVICE_STOPPED, SERVICE_STOP_PENDING,
    SERVICE_TABLE_ENTRYW,
};

const SERVICE_ALL_ACCESS: DWORD = 0xF01FF;
const SERVICE_AUTO_START: DWORD = 0x00000002;
const SERVICE_ERROR_NORMAL: DWORD = 0x00000000;
const SERVICE_WIN32_OWN_PROCESS: DWORD = 0x00000010;
const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;
const SERVICE_DELETE: DWORD = 0x00010000;
const SERVICE_START: DWORD = 0x00000010;

use crate::config::ServiceConfig;
use crate::demand::NativeMemoryTelemetry;
use crate::runtime::NativeTelemetryWorker;
use crate::service_host::{ServiceHost, ServiceState, ServiceWorker, StopSignal};

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

struct ServiceContext {
    stop: StopSignal,
    status_handle: SERVICE_STATUS_HANDLE,
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
            self.state
                .store(ServiceState::StopPending as u8, Ordering::Release);
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
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

pub fn install_service(config: &ServiceConfig) -> Result<(), String> {
    let registration = WindowsServiceRegistration::from_config(config)?;
    let manager_name = to_wide("");
    let manager = unsafe {
        OpenSCManagerW(
            manager_name.as_ptr(),
            std::ptr::null(),
            SC_MANAGER_CREATE_SERVICE,
        )
    };
    if manager.is_null() {
        return Err("OpenSCManagerW failed to open the service manager".to_owned());
    }

    let service_name = to_wide(&registration.service_name);
    let display_name = to_wide(&registration.display_name);
    let executable = to_wide(&registration.executable_path);
    let account = to_wide(&registration.service_account);
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
            std::ptr::null(),
        )
    };

    if service.is_null() {
        let os_error = std::io::Error::last_os_error();
        unsafe {
            winapi::um::winsvc::CloseServiceHandle(manager);
        }
        return Err(format!(
            "CreateServiceW failed for {} with error {os_error}",
            registration.service_name,
        ));
    }

    unsafe {
        let description = to_wide(&registration.description);
        let mut description_config = SERVICE_DESCRIPTIONW {
            lpDescription: description.as_ptr() as *mut u16,
        };
        if ChangeServiceConfig2W(
            service,
            SERVICE_CONFIG_DESCRIPTION,
            (&mut description_config as *mut SERVICE_DESCRIPTIONW).cast(),
        ) == FALSE
        {
            let error = std::io::Error::last_os_error();
            winapi::um::winsvc::CloseServiceHandle(service);
            winapi::um::winsvc::CloseServiceHandle(manager);
            return Err(format!("ChangeServiceConfig2W description failed: {error}"));
        }

        let mut actions = [
            SC_ACTION {
                Type: SC_ACTION_RESTART,
                Delay: 5_000,
            },
            SC_ACTION {
                Type: SC_ACTION_RESTART,
                Delay: 30_000,
            },
            SC_ACTION {
                Type: SC_ACTION_RESTART,
                Delay: 60_000,
            },
        ];
        let mut failure_actions = SERVICE_FAILURE_ACTIONSW {
            dwResetPeriod: 86_400,
            lpRebootMsg: std::ptr::null_mut(),
            lpCommand: std::ptr::null_mut(),
            cActions: actions.len() as DWORD,
            lpsaActions: actions.as_mut_ptr(),
        };
        if ChangeServiceConfig2W(
            service,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&mut failure_actions as *mut SERVICE_FAILURE_ACTIONSW).cast(),
        ) == FALSE
        {
            let error = std::io::Error::last_os_error();
            winapi::um::winsvc::CloseServiceHandle(service);
            winapi::um::winsvc::CloseServiceHandle(manager);
            return Err(format!("ChangeServiceConfig2W recovery failed: {error}"));
        }

        let mut failure_flag = SERVICE_FAILURE_ACTIONS_FLAG {
            fFailureActionsOnNonCrashFailures: TRUE,
        };
        if ChangeServiceConfig2W(
            service,
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            (&mut failure_flag as *mut SERVICE_FAILURE_ACTIONS_FLAG).cast(),
        ) == FALSE
        {
            let error = std::io::Error::last_os_error();
            winapi::um::winsvc::CloseServiceHandle(service);
            winapi::um::winsvc::CloseServiceHandle(manager);
            return Err(format!(
                "ChangeServiceConfig2W failure flag failed: {error}"
            ));
        }

        winapi::um::winsvc::CloseServiceHandle(service);
        winapi::um::winsvc::CloseServiceHandle(manager);
    }

    Ok(())
}

pub fn run_as_service() -> Result<bool, String> {
    let service_name = to_wide(crate::config::DEFAULT_SERVICE_NAME);
    let mut table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: service_name.as_ptr() as *mut u16,
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: std::ptr::null_mut(),
            lpServiceProc: None,
        },
    ];

    let started = unsafe { StartServiceCtrlDispatcherW(table.as_mut_ptr()) };
    if started != 0 {
        return Ok(true);
    }

    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) {
        Ok(false)
    } else {
        Err(format!("StartServiceCtrlDispatcherW failed: {error}"))
    }
}

unsafe extern "system" fn service_main(_argc: DWORD, _argv: *mut *mut u16) {
    let stop = StopSignal::new();
    let service_name = to_wide(crate::config::DEFAULT_SERVICE_NAME);
    let mut context = Box::new(ServiceContext {
        stop: stop.clone(),
        status_handle: std::ptr::null_mut(),
    });
    let status_handle = RegisterServiceCtrlHandlerExW(
        service_name.as_ptr(),
        Some(service_control_handler),
        (&mut *context) as *mut ServiceContext as *mut _,
    );
    if status_handle.is_null() {
        return;
    }
    context.status_handle = status_handle;

    let _ = publish_status(
        status_handle,
        ScmServiceStatus::from_state(ServiceState::StartPending),
    );
    let config = match ServiceConfig::load_default() {
        Ok(config) => config,
        Err(_) => {
            let _ = publish_status(
                status_handle,
                ScmServiceStatus::from_state(ServiceState::Failed),
            );
            return;
        }
    };
    let shutdown_timeout = config.shutdown_timeout;
    let poll_interval = config.poll_interval;
    let status_handle_value = status_handle as usize;
    let mut host = ServiceHost::with_stop_and_shutdown_timeout(
        move |service_stop: &StopSignal| {
            let mut worker = NativeTelemetryWorker::new(NativeMemoryTelemetry, poll_interval)
                .map_err(|error| format!("construct native telemetry worker: {error}"))?;
            worker
                .initialize(service_stop)
                .map_err(|error| format!("initialize native telemetry worker: {error}"))?;
            let _ = publish_status(
                status_handle_value as SERVICE_STATUS_HANDLE,
                ScmServiceStatus::from_state(ServiceState::Running),
            );
            worker.run(service_stop)
        },
        stop,
        shutdown_timeout,
    );
    let result = host.run();
    let final_state = if result.is_ok() {
        ServiceState::Stopped
    } else {
        ServiceState::Failed
    };
    let _ = publish_status(status_handle, ScmServiceStatus::from_state(final_state));
}

unsafe extern "system" fn service_control_handler(
    control: DWORD,
    _event_type: DWORD,
    _event_data: *mut c_void,
    context: *mut c_void,
) -> DWORD {
    if context.is_null() {
        return 1;
    }
    let context = &*(context as *const ServiceContext);
    if control == SERVICE_CONTROL_STOP || control == SERVICE_CONTROL_SHUTDOWN {
        context.stop.cancel();
        let _ = publish_status(
            context.status_handle,
            ScmServiceStatus::from_state(ServiceState::StopPending),
        );
        0
    } else {
        1
    }
}

fn publish_status(handle: SERVICE_STATUS_HANDLE, status: ScmServiceStatus) -> Result<(), String> {
    let mut native = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: status.current_state,
        dwControlsAccepted: status.controls_accepted,
        dwWin32ExitCode: status.exit_code,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: 0,
    };
    let published = unsafe { SetServiceStatus(handle, &mut native) };
    if published == 0 {
        Err(format!(
            "SetServiceStatus failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
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

pub fn start_service(service_name: &str) -> Result<(), String> {
    let manager_name = to_wide("");
    let manager = unsafe { OpenSCManagerW(manager_name.as_ptr(), std::ptr::null(), 0) };
    if manager.is_null() {
        return Err("OpenSCManagerW failed while preparing to start the service".to_owned());
    }

    let service_name_u16 = to_wide(service_name);
    let service = unsafe { OpenServiceW(manager, service_name_u16.as_ptr(), SERVICE_START) };
    if service.is_null() {
        unsafe {
            winapi::um::winsvc::CloseServiceHandle(manager);
        }
        return Err(format!("OpenServiceW failed for {service_name}"));
    }

    let started = unsafe { StartServiceW(service, 0, std::ptr::null_mut()) };
    unsafe {
        winapi::um::winsvc::CloseServiceHandle(service);
        winapi::um::winsvc::CloseServiceHandle(manager);
    }

    if started == FALSE {
        return Err(format!("StartServiceW failed for {service_name}"));
    }

    Ok(())
}

pub fn remove_service(service_name: &str) -> Result<(), String> {
    let manager_name = to_wide("");
    let manager = unsafe { OpenSCManagerW(manager_name.as_ptr(), std::ptr::null(), 0) };
    if manager.is_null() {
        return Err("OpenSCManagerW failed while preparing to remove the service".to_owned());
    }

    let service_name_u16 = to_wide(service_name);
    let service = unsafe { OpenServiceW(manager, service_name_u16.as_ptr(), SERVICE_DELETE) };
    if service.is_null() {
        unsafe {
            winapi::um::winsvc::CloseServiceHandle(manager);
        }
        return Err(format!("OpenServiceW failed for {service_name}"));
    }

    let removed = unsafe { DeleteService(service) };
    unsafe {
        winapi::um::winsvc::CloseServiceHandle(service);
        winapi::um::winsvc::CloseServiceHandle(manager);
    }

    if removed == FALSE {
        return Err(format!("DeleteService failed for {service_name}"));
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
