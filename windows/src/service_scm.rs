use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicU8, Ordering};

use winapi::ctypes::c_void;
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
use winapi::um::winnt::SERVICE_ERROR_NORMAL;
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
const SERVICE_WIN32_OWN_PROCESS: DWORD = 0x00000010;
const ERROR_FAILED_SERVICE_CONTROLLER_CONNECT: i32 = 1063;
const SERVICE_DELETE: DWORD = 0x00010000;
const SERVICE_START: DWORD = 0x00000010;
const MAX_SERVICE_NAME_UTF16_UNITS: usize = 256;

use crate::config::ServiceConfig;
use crate::demand::{
    process_session_id, AtomicRawTelemetryPublisher, NativeMemoryTelemetry, SystemTelemetryClock,
};
use crate::event_log::{
    ServiceEvent, ServiceEventId, ServiceEventLevel, ServiceEventSink, WindowsEventLog,
};
use crate::runtime::RawTelemetryWorker;
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
    pub error_control: DWORD,
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
            error_control: SERVICE_ERROR_NORMAL,
        })
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

pub fn install_service(config: &ServiceConfig) -> Result<(), String> {
    let registration = WindowsServiceRegistration::from_config(config)?;
    provision_program_data_acl(config)?;
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
            registration.error_control,
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

fn provision_program_data_acl(config: &ServiceConfig) -> Result<(), String> {
    use winapi::shared::minwindef::HLOCAL;
    use winapi::shared::sddl::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use winapi::um::securitybaseapi::SetFileSecurityW;
    use winapi::um::winbase::LocalFree;
    use winapi::um::winnt::{
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    };

    if !config
        .service_account
        .eq_ignore_ascii_case(r"NT AUTHORITY\LocalService")
    {
        return Err(
            "automatic ProgramData ACL provisioning supports only NT AUTHORITY\\LocalService"
                .to_owned(),
        );
    }
    let report_parent = std::path::Path::new(&config.demand_report_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "demand report path has no parent directory".to_owned())?;
    let config_parent = std::path::Path::new(&config.config_path)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "configuration path has no parent directory".to_owned())?;
    if report_parent != config_parent {
        return Err("configuration and telemetry files must share one ACL root".to_owned());
    }
    std::fs::create_dir_all(report_parent)
        .map_err(|error| format!("create service data directory: {error}"))?;

    // Protected DACL: SYSTEM and Administrators have full control; LocalService
    // has the file/directory rights required to create and atomically replace
    // telemetry while no broad Users/Everyone ACE is inherited.
    let sddl = to_wide("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;LS)");
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: The SDDL and path are NUL-terminated UTF-16 buffers, the API
    // initializes descriptor, and LocalFree releases the returned allocation.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1.into(),
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == FALSE {
        return Err(format!(
            "convert ProgramData security descriptor: {}",
            std::io::Error::last_os_error()
        ));
    }
    let path = to_wide(&report_parent.to_string_lossy());
    let applied = unsafe {
        SetFileSecurityW(
            path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    unsafe {
        LocalFree(descriptor as HLOCAL);
    }
    if applied == FALSE {
        return Err(format!(
            "apply ProgramData security descriptor: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub fn run_as_service() -> Result<bool, String> {
    // This is an own-process service, so SCM ignores the table name and passes
    // the installed identity to `service_main`. Keeping the table entry empty
    // avoids coupling dispatcher attachment to the configurable install name.
    let service_name = to_wide("");
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

unsafe extern "system" fn service_main(argc: DWORD, argv: *mut *mut u16) {
    let event_source = match service_name_from_main_args(argc, argv) {
        Ok(service_name) => service_name,
        Err(error) => {
            eprintln!("resolve SCM service identity failed: {error}");
            return;
        }
    };
    let stop = StopSignal::new();
    let service_name = to_wide(&event_source);
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
        emit_event(
            &event_source,
            ServiceEvent::new(
                ServiceEventId::StatusPublishFailed,
                ServiceEventLevel::Error,
                format!(
                    "RegisterServiceCtrlHandlerExW failed: {}",
                    std::io::Error::last_os_error()
                ),
            ),
        );
        return;
    }
    context.status_handle = status_handle;

    if let Err(error) = publish_status(
        status_handle,
        ScmServiceStatus::from_state(ServiceState::StartPending),
    ) {
        emit_event(
            &event_source,
            ServiceEvent::new(
                ServiceEventId::StatusPublishFailed,
                ServiceEventLevel::Error,
                error,
            ),
        );
        return;
    }
    emit_event(
        &event_source,
        ServiceEvent::new(
            ServiceEventId::StartPending,
            ServiceEventLevel::Information,
            "service initialization started",
        ),
    );
    let config = match ServiceConfig::load_default() {
        Ok(config) => config,
        Err(error) => {
            emit_event(
                &event_source,
                ServiceEvent::new(
                    ServiceEventId::ConfigurationFailed,
                    ServiceEventLevel::Error,
                    format!("service configuration failed: {error}"),
                ),
            );
            let _ = publish_status(
                status_handle,
                ScmServiceStatus::from_state(ServiceState::Failed),
            );
            return;
        }
    };
    if let Err(error) = validate_configured_service_identity(&config.service_name, &event_source) {
        emit_event(
            &event_source,
            ServiceEvent::new(
                ServiceEventId::ConfigurationFailed,
                ServiceEventLevel::Error,
                error,
            ),
        );
        let _ = publish_status(
            status_handle,
            ScmServiceStatus::from_state(ServiceState::Failed),
        );
        return;
    }
    let shutdown_timeout = config.shutdown_timeout;
    let poll_interval = config.poll_interval;
    let vm_name = config.vm_name;
    let service_name = config.service_name;
    let telemetry_path = config.demand_report_path;
    let status_handle_value = status_handle as usize;
    let running_event_source = event_source.clone();
    let stop_observer = stop.clone();
    let mut host = ServiceHost::with_stop_and_shutdown_timeout(
        move |service_stop: &StopSignal| {
            let session_id = process_session_id(&service_name)
                .map_err(|error| format!("runtime wiring / session identity: {error}"))?;
            let mut worker = RawTelemetryWorker::new(
                NativeMemoryTelemetry,
                AtomicRawTelemetryPublisher::new(&telemetry_path),
                SystemTelemetryClock::default(),
                &vm_name,
                &service_name,
                session_id,
                poll_interval,
            )
            .map_err(|error| format!("runtime wiring / worker construction: {error}"))?;
            worker
                .initialize(service_stop)
                .map_err(|error| format!("runtime wiring / worker initialization: {error}"))?;
            publish_status(
                status_handle_value as SERVICE_STATUS_HANDLE,
                ScmServiceStatus::from_state(ServiceState::Running),
            )
            .map_err(|error| format!("runtime wiring / publish running status: {error}"))?;
            emit_event(
                &running_event_source,
                ServiceEvent::new(
                    ServiceEventId::Running,
                    ServiceEventLevel::Information,
                    "service worker is running",
                ),
            );
            worker.run(service_stop)
        },
        stop,
        shutdown_timeout,
    );
    let result = host
        .run()
        .map_err(|error| format!("runtime wiring / service host execution: {error}"));
    let stop_requested = stop_observer.is_cancelled();
    if stop_requested {
        emit_event(
            &event_source,
            ServiceEvent::new(
                ServiceEventId::StopRequested,
                ServiceEventLevel::Information,
                "service cancellation was requested",
            ),
        );
    }
    let (final_state, event) = classify_service_exit(result, stop_requested);
    emit_event(&event_source, event);
    if let Err(error) = publish_status(status_handle, ScmServiceStatus::from_state(final_state)) {
        emit_event(
            &event_source,
            ServiceEvent::new(
                ServiceEventId::StatusPublishFailed,
                ServiceEventLevel::Error,
                error,
            ),
        );
    }
}

fn validate_configured_service_identity(configured: &str, scm: &str) -> Result<(), String> {
    if configured == scm {
        Ok(())
    } else {
        Err(format!(
            "configured service name {configured} does not match SCM identity {scm}"
        ))
    }
}

unsafe fn service_name_from_main_args(argc: DWORD, argv: *mut *mut u16) -> Result<String, String> {
    if argc == 0 || argv.is_null() {
        return Err("SCM did not provide the service name argument".to_owned());
    }
    let service_name = *argv;
    if service_name.is_null() {
        return Err("SCM provided a null service name argument".to_owned());
    }
    let mut length = 0;
    while length <= MAX_SERVICE_NAME_UTF16_UNITS {
        if *service_name.add(length) == 0 {
            if length == 0 {
                return Err("SCM provided an empty service name argument".to_owned());
            }
            return String::from_utf16(std::slice::from_raw_parts(service_name, length))
                .map_err(|error| format!("SCM service name is invalid UTF-16: {error}"));
        }
        length += 1;
    }
    Err(format!(
        "SCM service name exceeds {MAX_SERVICE_NAME_UTF16_UNITS} UTF-16 units"
    ))
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
        if publish_status(
            context.status_handle,
            ScmServiceStatus::from_state(ServiceState::StopPending),
        )
        .is_err()
        {
            return 1;
        }
        0
    } else {
        1
    }
}

fn classify_service_exit(
    result: Result<(), String>,
    stop_requested: bool,
) -> (ServiceState, ServiceEvent) {
    match (result, stop_requested) {
        (Ok(()), true) => (
            ServiceState::Stopped,
            ServiceEvent::new(
                ServiceEventId::Stopped,
                ServiceEventLevel::Information,
                "service stopped after cancellation",
            ),
        ),
        (Ok(()), false) => (
            ServiceState::Failed,
            ServiceEvent::new(
                ServiceEventId::UnexpectedExit,
                ServiceEventLevel::Error,
                "service worker exited without a stop request",
            ),
        ),
        (Err(error), _) => (
            ServiceState::Failed,
            ServiceEvent::new(
                ServiceEventId::WorkerFailed,
                ServiceEventLevel::Error,
                error,
            ),
        ),
    }
}

fn emit_event(source: &str, event: ServiceEvent) {
    let result = WindowsEventLog::open(source).and_then(|event_log| event_log.emit(&event));
    if let Err(error) = result {
        eprintln!(
            "Windows Event Log publication failed for event {}: {error}",
            event.id as u32
        );
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
    fn registration_uses_normal_startup_error_control() {
        let registration = WindowsServiceRegistration::from_config(&ServiceConfig {
            vm_name: "test-vm".to_owned(),
            service_name: "TestService".to_owned(),
            display_name: "Test service".to_owned(),
            description: "Test configuration".to_owned(),
            demand_report_path: r"C:\test\telemetry.jsonl".to_owned(),
            service_account: r"NT AUTHORITY\LocalService".to_owned(),
            config_path: r"C:\test\config.json".to_owned(),
            poll_interval: std::time::Duration::from_millis(20),
            shutdown_timeout: std::time::Duration::from_millis(30),
        })
        .expect("registration should be valid");

        assert_eq!(registration.error_control, 1);
    }

    #[test]
    fn service_main_uses_the_scm_supplied_service_identity() {
        let mut encoded = to_wide("ConfiguredVirtioMemService");
        let mut argument = encoded.as_mut_ptr();

        let service_name = unsafe { service_name_from_main_args(1, &mut argument) };

        assert_eq!(service_name.as_deref(), Ok("ConfiguredVirtioMemService"));
        assert_eq!(
            validate_configured_service_identity(
                "ConfiguredVirtioMemService",
                service_name.as_deref().expect("service identity")
            ),
            Ok(())
        );
        assert!(validate_configured_service_identity(
            "AnotherService",
            service_name.as_deref().expect("service identity")
        )
        .is_err());
    }

    #[test]
    fn service_main_rejects_missing_or_unbounded_service_identity() {
        assert!(unsafe { service_name_from_main_args(0, std::ptr::null_mut()) }.is_err());

        let mut encoded = vec![u16::from(b'a'); MAX_SERVICE_NAME_UTF16_UNITS + 1];
        let mut argument = encoded.as_mut_ptr();
        assert!(unsafe { service_name_from_main_args(1, &mut argument) }.is_err());
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

    #[test]
    fn intentional_stop_is_successful_and_unexpected_exit_is_recoverable() {
        let (state, event) = classify_service_exit(Ok(()), true);
        assert_eq!(state, ServiceState::Stopped);
        assert_eq!(event.id, ServiceEventId::Stopped);
        assert_eq!(event.level, ServiceEventLevel::Information);

        let (state, event) = classify_service_exit(Ok(()), false);
        assert_eq!(state, ServiceState::Failed);
        assert_eq!(event.id, ServiceEventId::UnexpectedExit);
        assert_eq!(event.level, ServiceEventLevel::Error);
        assert_eq!(ScmServiceStatus::from_state(state).exit_code, 1);
    }

    #[test]
    fn worker_failure_is_recoverable_even_after_stop_was_requested() {
        let (state, event) = classify_service_exit(Err("poll failed".to_owned()), true);

        assert_eq!(state, ServiceState::Failed);
        assert_eq!(event.id, ServiceEventId::WorkerFailed);
        assert_eq!(event.message, "poll failed");
        assert_eq!(ScmServiceStatus::from_state(state).exit_code, 1);
    }
}
