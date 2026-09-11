use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

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
struct StopState {
    cancelled: AtomicBool,
    wake: Condvar,
    lock: Mutex<()>,
}

#[derive(Debug, Clone)]
pub struct StopSignal {
    state: Arc<StopState>,
}

impl StopSignal {
    pub fn new() -> Self {
        Self {
            state: Arc::new(StopState {
                cancelled: AtomicBool::new(false),
                wake: Condvar::new(),
                lock: Mutex::new(()),
            }),
        }
    }

    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        self.state.wake.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    pub fn wait(&self, timeout: Duration) {
        if self.is_cancelled() {
            return;
        }

        let guard = match self.state.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _ = self.state.wake.wait_timeout(guard, timeout);
    }

    fn wait_until_cancelled(&self) {
        let mut guard = match self.state.lock.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        while !self.is_cancelled() {
            guard = match self.state.wake.wait(guard) {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
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
    worker: Option<W>,
    stop: StopSignal,
    state: AtomicU8,
    shutdown_timeout: Duration,
}

impl<W> ServiceHost<W>
where
    W: ServiceWorker + Send + 'static,
{
    pub fn with_shutdown_timeout(worker: W, shutdown_timeout: Duration) -> Self {
        Self::with_stop_and_shutdown_timeout(worker, StopSignal::new(), shutdown_timeout)
    }

    pub fn with_stop_and_shutdown_timeout(
        worker: W,
        stop: StopSignal,
        shutdown_timeout: Duration,
    ) -> Self {
        Self {
            worker: Some(worker),
            stop,
            state: AtomicU8::new(ServiceState::Created as u8),
            shutdown_timeout,
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

        if self.shutdown_timeout.is_zero() {
            self.state
                .store(ServiceState::Failed as u8, Ordering::Release);
            return Err(ServiceHostError::InvalidShutdownTimeout);
        }

        self.state
            .store(ServiceState::StartPending as u8, Ordering::Release);
        let Some(worker) = self.worker.as_mut() else {
            self.state
                .store(ServiceState::Failed as u8, Ordering::Release);
            return Err(ServiceHostError::Startup(
                "service worker is unavailable".to_owned(),
            ));
        };
        if let Err(error) = worker.initialize(&self.stop) {
            self.state
                .store(ServiceState::Failed as u8, Ordering::Release);
            return Err(ServiceHostError::Startup(error));
        }
        self.state
            .store(ServiceState::Running as u8, Ordering::Release);
        let Some(mut worker) = self.worker.take() else {
            self.state
                .store(ServiceState::Failed as u8, Ordering::Release);
            return Err(ServiceHostError::Startup(
                "service worker is unavailable".to_owned(),
            ));
        };
        let stop = self.stop.clone();
        enum HostEvent {
            Worker(Result<(), String>),
            StopRequested,
        }
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        let result_sender = event_sender.clone();
        let _worker_thread = std::thread::spawn(move || {
            let result = worker.run(&stop);
            let _ = result_sender.send(HostEvent::Worker(result));
            stop.cancel();
        });
        let stop = self.stop.clone();
        let _stop_thread = std::thread::spawn(move || {
            stop.wait_until_cancelled();
            let _ = event_sender.send(HostEvent::StopRequested);
        });

        let mut shutdown_started = None;
        loop {
            let event = match shutdown_started {
                Some(started) => event_receiver
                    .recv_timeout(self.shutdown_timeout.saturating_sub(started.elapsed())),
                None => event_receiver
                    .recv()
                    .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
            };
            match event {
                Ok(HostEvent::Worker(Ok(()))) => {
                    self.state
                        .store(ServiceState::Stopped as u8, Ordering::Release);
                    return Ok(());
                }
                Ok(HostEvent::Worker(Err(error))) => {
                    self.state
                        .store(ServiceState::Failed as u8, Ordering::Release);
                    return Err(ServiceHostError::Worker(error));
                }
                Ok(HostEvent::StopRequested) => {
                    shutdown_started.get_or_insert_with(Instant::now);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    self.state
                        .store(ServiceState::Failed as u8, Ordering::Release);
                    return Err(ServiceHostError::Worker(
                        "service worker exited without reporting a result".to_owned(),
                    ));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    self.state
                        .store(ServiceState::Failed as u8, Ordering::Release);
                    return Err(ServiceHostError::ShutdownTimeout);
                }
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
        let mut host = ServiceHost::with_shutdown_timeout(
            |stop: &StopSignal| {
                assert!(!stop.is_cancelled());
                Ok(())
            },
            Duration::from_secs(1),
        );

        assert_eq!(host.state(), ServiceState::Created);
        host.run().expect("worker should exit successfully");
        assert_eq!(host.state(), ServiceState::Stopped);
    }

    #[test]
    fn records_worker_failure() {
        let mut host = ServiceHost::with_shutdown_timeout(
            |_stop: &StopSignal| Err("poll failed".to_owned()),
            Duration::from_secs(1),
        );

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
        let mut host = ServiceHost::with_shutdown_timeout(FailingWorker, Duration::from_secs(1));

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
        let mut host = ServiceHost::with_shutdown_timeout(
            |stop: &StopSignal| {
                assert!(stop.is_cancelled());
                Ok(())
            },
            Duration::from_secs(1),
        );
        host.request_stop();

        host.run().expect("stopped worker should exit successfully");
    }

    #[test]
    fn rejects_running_host_reentry() {
        let mut host =
            ServiceHost::with_shutdown_timeout(|_stop: &StopSignal| Ok(()), Duration::from_secs(1));
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

    #[test]
    fn rejects_zero_shutdown_timeout() {
        let mut host =
            ServiceHost::with_shutdown_timeout(|_stop: &StopSignal| Ok(()), Duration::ZERO);

        assert_eq!(host.run(), Err(ServiceHostError::InvalidShutdownTimeout));
        assert_eq!(host.state(), ServiceState::Failed);
    }

    #[test]
    fn returns_shutdown_timeout_for_worker_that_does_not_stop() {
        let stop = StopSignal::new();
        let cancel = stop.clone();
        let _canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            cancel.cancel();
        });
        let mut host = ServiceHost::with_stop_and_shutdown_timeout(
            |_stop: &StopSignal| {
                thread::sleep(Duration::from_millis(200));
                Ok(())
            },
            stop,
            Duration::from_millis(20),
        );

        assert_eq!(host.run(), Err(ServiceHostError::ShutdownTimeout));
        assert_eq!(host.state(), ServiceState::Failed);
    }

    #[test]
    fn waits_for_worker_to_finish_within_shutdown_timeout() {
        let stop = StopSignal::new();
        let cancel = stop.clone();
        let _canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            cancel.cancel();
        });
        let mut host = ServiceHost::with_stop_and_shutdown_timeout(
            |stop: &StopSignal| {
                while !stop.is_cancelled() {
                    thread::yield_now();
                }
                Ok(())
            },
            stop,
            Duration::from_secs(1),
        );

        assert!(host.run().is_ok());
        assert_eq!(host.state(), ServiceState::Stopped);
    }
}
