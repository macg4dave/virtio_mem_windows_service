use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::error::ServiceHostError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ServiceState {
    Created,
    StartPending,
    Running,
    StopPending,
    Stopped,
    Failed,
}

impl ServiceState {
    pub(crate) fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Created,
            1 => Self::StartPending,
            2 => Self::Running,
            3 => Self::StopPending,
            4 => Self::Stopped,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug)]
pub struct StopSignal {
    cancelled: AtomicBool,
    wake: Condvar,
    lock: Mutex<()>,
}

impl StopSignal {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            wake: Condvar::new(),
            lock: Mutex::new(()),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.wake.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn wait(&self, timeout: Duration) {
        if self.is_cancelled() {
            return;
        }

        let guard = match self.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _ = self.wake.wait_timeout(guard, timeout);
    }
}

impl Default for StopSignal {
    fn default() -> Self {
        Self::new()
    }
}

pub trait ServiceWorker {
    fn initialize(&mut self, _stop: &StopSignal) -> Result<(), String> {
        Ok(())
    }

    fn run(&mut self, stop: &StopSignal) -> Result<(), String>;
}

impl<F> ServiceWorker for F
where
    F: FnMut(&StopSignal) -> Result<(), String>,
{
    fn run(&mut self, stop: &StopSignal) -> Result<(), String> {
        self(stop)
    }
}

pub struct ServiceHost<W> {
    worker: W,
    stop: StopSignal,
    state: AtomicU8,
}

impl<W> ServiceHost<W>
where
    W: ServiceWorker,
{
    pub fn new(worker: W) -> Self {
        Self {
            worker,
            stop: StopSignal::new(),
            state: AtomicU8::new(ServiceState::Created as u8),
        }
    }

    pub fn state(&self) -> ServiceState {
        ServiceState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn request_stop(&self) {
        let _ = self.state.compare_exchange(
            ServiceState::Running as u8,
            ServiceState::StopPending as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.stop.cancel();
    }

    pub fn run(&mut self) -> Result<(), ServiceHostError> {
        match self.state() {
            ServiceState::Created => {}
            ServiceState::StartPending | ServiceState::Running | ServiceState::StopPending => {
                return Err(ServiceHostError::AlreadyRunning)
            }
            ServiceState::Stopped | ServiceState::Failed => {
                return Err(ServiceHostError::AlreadyStopped)
            }
        }

        self.state
            .store(ServiceState::StartPending as u8, Ordering::Release);
        if let Err(error) = self.worker.initialize(&self.stop) {
            self.state
                .store(ServiceState::Failed as u8, Ordering::Release);
            return Err(ServiceHostError::Startup(error));
        }
        self.state
            .store(ServiceState::Running as u8, Ordering::Release);
        match self.worker.run(&self.stop) {
            Ok(()) => {
                self.state
                    .store(ServiceState::Stopped as u8, Ordering::Release);
                Ok(())
            }
            Err(error) => {
                self.state
                    .store(ServiceState::Failed as u8, Ordering::Release);
                Err(ServiceHostError::Worker(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn transitions_to_stopped_after_worker_exits() {
        let mut host = ServiceHost::new(|stop: &StopSignal| {
            assert!(!stop.is_cancelled());
            Ok(())
        });

        assert_eq!(host.state(), ServiceState::Created);
        host.run().expect("worker should exit successfully");
        assert_eq!(host.state(), ServiceState::Stopped);
    }

    #[test]
    fn records_worker_failure() {
        let mut host = ServiceHost::new(|_stop: &StopSignal| Err("poll failed".to_owned()));

        assert_eq!(
            host.run(),
            Err(ServiceHostError::Worker("poll failed".to_owned()))
        );
        assert_eq!(host.state(), ServiceState::Failed);
    }

    struct FailingWorker;

    impl ServiceWorker for FailingWorker {
        fn initialize(&mut self, _stop: &StopSignal) -> Result<(), String> {
            Err("configuration invalid".to_owned())
        }

        fn run(&mut self, _stop: &StopSignal) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn records_startup_failure_before_running() {
        let mut host = ServiceHost::new(FailingWorker);

        assert_eq!(
            host.run(),
            Err(ServiceHostError::Startup(
                "configuration invalid".to_owned()
            ))
        );
        assert_eq!(host.state(), ServiceState::Failed);
    }

    #[test]
    fn exposes_stop_request_to_worker() {
        let mut host = ServiceHost::new(|stop: &StopSignal| {
            assert!(stop.is_cancelled());
            Ok(())
        });
        host.request_stop();

        host.run().expect("stopped worker should exit successfully");
    }

    #[test]
    fn rejects_running_host_reentry() {
        let mut host = ServiceHost::new(|_stop: &StopSignal| Ok(()));
        host.state
            .store(ServiceState::Running as u8, Ordering::Release);

        assert_eq!(host.run(), Err(ServiceHostError::AlreadyRunning));
    }

    #[test]
    fn cancellation_wakes_waiters() {
        let stop = Arc::new(StopSignal::new());
        let waiter = Arc::clone(&stop);
        let thread = thread::spawn(move || waiter.wait(Duration::from_secs(60)));

        stop.cancel();
        thread
            .join()
            .expect("waiter should exit after cancellation");
    }
}
