use std::sync::atomic::{AtomicBool, Ordering};
use virtio_mem_core::{
    bytes_to_kibibytes, parse_virtio_mem_xml_for_alias, VirtioMemState, MIN_HEADROOM_BYTES,
};

use crate::compatibility_source::{CompatibilitySource, FixedCompatibilitySource};
use crate::runtime::{ResizeSink, ResizeSinkError};
use crate::virsh::VirshCommand;

pub struct VirshResizeSink<C, E = FixedCompatibilitySource> {
    command: C,
    vm_name: String,
    alias: String,
    compatibility_source: E,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PreparedResize {
    arguments: Vec<String>,
    current_bytes: u64,
    target_bytes: u64,
}

impl PreparedResize {
    pub(crate) fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(crate) fn current_bytes(&self) -> u64 {
        self.current_bytes
    }

    pub(crate) fn target_bytes(&self) -> u64 {
        self.target_bytes
    }
}

impl<C> VirshResizeSink<C, FixedCompatibilitySource> {
    pub fn new(command: C, vm_name: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
            alias: alias.into(),
            compatibility_source: FixedCompatibilitySource::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_external_compatibility(
        mut self,
        compatibility: virtio_mem_core::VirtioMemCompatibility,
    ) -> Self {
        self.compatibility_source = FixedCompatibilitySource::new(compatibility);
        self
    }
}

impl<C, E> VirshResizeSink<C, E> {
    /// Replace static compatibility evidence with a live or injected source.
    pub fn with_compatibility_source<N>(self, compatibility_source: N) -> VirshResizeSink<C, N> {
        VirshResizeSink {
            command: self.command,
            vm_name: self.vm_name,
            alias: self.alias,
            compatibility_source,
        }
    }
}

impl<C: VirshCommand, E: CompatibilitySource> ResizeSink for VirshResizeSink<C, E> {
    fn request_resize_guarded(
        &self,
        expected: VirtioMemState,
        target: u64,
        stop: &AtomicBool,
        authorize: &dyn Fn() -> Result<(), String>,
    ) -> Result<(), ResizeSinkError> {
        let prepared = self
            .prepare(target, |state| {
                if *state != expected || state.requested_bytes != state.current_bytes {
                    return Err("live device state changed since the resize decision".to_owned());
                }
                Ok(())
            })
            .map_err(ResizeSinkError::Rejected)?;
        authorize().map_err(ResizeSinkError::Rejected)?;
        self.apply_unless_stopped(prepared, stop)
    }

    fn settle_pending_guarded(
        &self,
        expected: VirtioMemState,
        stop: &AtomicBool,
    ) -> Result<(), ResizeSinkError> {
        let prepared = self
            .prepare(expected.current_bytes, |state| {
                if *state != expected || state.requested_bytes == state.current_bytes {
                    return Err("live pending resize changed since the settle decision".to_owned());
                }
                Ok(())
            })
            .map_err(ResizeSinkError::Rejected)?;
        self.apply_unless_stopped(prepared, stop)
    }

    fn widen_usable_region_guarded(
        &self,
        expected: VirtioMemState,
        stop: &AtomicBool,
    ) -> Result<(), ResizeSinkError> {
        let converged = |state: &VirtioMemState| {
            if *state != expected || state.requested_bytes != state.current_bytes {
                return Err("live device changed since the widening decision".to_owned());
            }
            Ok(())
        };
        // The largest valid target; QEMU adds its extent above it.
        let ceiling = expected.size_bytes.saturating_sub(MIN_HEADROOM_BYTES);
        let widen = self
            .prepare(
                ceiling - ceiling % expected.block_size_bytes.max(1),
                converged,
            )
            .map_err(ResizeSinkError::Rejected)?;
        let restore = self
            .prepare(expected.requested_bytes, converged)
            .map_err(ResizeSinkError::Rejected)?;
        if stop.load(Ordering::Acquire) {
            return Err(ResizeSinkError::Rejected("shutdown requested".to_owned()));
        }
        let domain = |operation: &str| {
            self.command
                .run(&[operation.to_owned(), self.vm_name.clone()])
                .map(|_| ())
                .map_err(|error| error.to_string())
        };
        domain("suspend").map_err(ResizeSinkError::Rejected)?;
        // The guest cannot act on the device while paused. Never resume with
        // the whole device possibly requested: the driver would plug all of
        // it. A failed widening command may still have applied, so the
        // restore always runs.
        let widened = self.apply_prepared(widen);
        let restored = self
            .apply_prepared(restore)
            .or_else(|_| self.apply_prepared(self.prepare_restore(expected)?));
        if let Err(error) = restored {
            return Err(ResizeSinkError::CommandUnknown(format!(
                "{WIDEN_UNRESTORED}: {error}"
            )));
        }
        match (widened, domain("resume")) {
            (Ok(()), Ok(())) => Ok(()),
            (_, Err(error)) => Err(ResizeSinkError::CommandUnknown(format!(
                "domain could not be resumed after widening: {error}"
            ))),
            (Err(error), Ok(())) => Err(ResizeSinkError::CommandUnknown(format!(
                "widening the usable region failed after pausing: {error}"
            ))),
        }
    }
}

/// Prefix of a widening failure that left the whole device requested; the
/// domain is deliberately left paused for operator recovery.
const WIDEN_UNRESTORED: &str = "requested size was not restored after widening; domain left paused";

impl<C: VirshCommand, E: CompatibilitySource> VirshResizeSink<C, E> {
    /// The restoring command for a widened device, built without the live
    /// precondition (the device then requests its whole size).
    fn prepare_restore(&self, expected: VirtioMemState) -> Result<PreparedResize, String> {
        self.prepare(expected.requested_bytes, |state| {
            if state.current_bytes != expected.current_bytes {
                return Err("device allocation changed while paused".to_owned());
            }
            Ok(())
        })
    }

    pub(crate) fn prepare_resize(&self, requested_bytes: u64) -> Result<PreparedResize, String> {
        self.prepare(requested_bytes, |state| {
            if state.requested_bytes != state.current_bytes {
                return Err(
                    "refusing resize while the previous request has not converged".to_owned(),
                );
            }
            Ok(())
        })
    }

    pub(crate) fn apply_prepared(&self, prepared: PreparedResize) -> Result<(), String> {
        self.command
            .run(prepared.arguments())
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn apply_unless_stopped(
        &self,
        prepared: PreparedResize,
        stop: &AtomicBool,
    ) -> Result<(), ResizeSinkError> {
        if stop.load(Ordering::Acquire) {
            return Err(ResizeSinkError::Rejected("shutdown requested".to_owned()));
        }
        self.apply_prepared(prepared)
            .map_err(ResizeSinkError::CommandUnknown)
    }

    /// Prepares the one-shot abandon-to-current recovery after two
    /// independent observations have qualified `stable_current_bytes`.
    /// This immediate read is the final race check before an operator applies
    /// the command.
    pub(crate) fn prepare_abandon_to_current(
        &self,
        immutable_target_bytes: u64,
        expected: VirtioMemState,
    ) -> Result<PreparedResize, String> {
        self.prepare(expected.current_bytes, |state| {
            if *state != expected
                || state.requested_bytes != immutable_target_bytes
                || state.current_bytes <= immutable_target_bytes
            {
                return Err(
                    "live state changed before abandon-to-current recovery could be applied"
                        .to_owned(),
                );
            }
            Ok(())
        })
    }

    /// Reads live XML immediately before a command, validates compatibility and
    /// the target, then applies the operation's own live-state precondition.
    /// The fresh read is the race check; callers must not reuse an older one.
    fn prepare(
        &self,
        target_bytes: u64,
        precondition: impl FnOnce(&VirtioMemState) -> Result<(), String>,
    ) -> Result<PreparedResize, String> {
        let compatibility = self.compatibility_source.compatibility()?;
        let snapshot = self
            .command
            .run(&["dumpxml".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let snapshot = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?;
        snapshot
            .compatibility
            .merge(compatibility)
            .map_err(|error| error.to_string())?
            .validate_for_resize()
            .map_err(|error| error.to_string())?;
        let state = snapshot.memory;
        state
            .validate_target(target_bytes)
            .map_err(|error| error.to_string())?;
        precondition(&state)?;
        let requested_kib = bytes_to_kibibytes(target_bytes)
            .ok_or_else(|| "resize target must be an integer number of KiB".to_owned())?;
        Ok(PreparedResize {
            arguments: vec![
                "update-memory-device".to_owned(),
                self.vm_name.clone(),
                "--alias".to_owned(),
                self.alias.clone(),
                "--requested-size".to_owned(),
                requested_kib.to_string(),
                "--live".to_owned(),
            ],
            current_bytes: state.current_bytes,
            target_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::virsh::{VirshCommand, VirshError};
    use virtio_mem_core::VirtioMemCompatibility;

    struct DriftedAttestation;

    impl CompatibilitySource for DriftedAttestation {
        fn compatibility(&self) -> Result<VirtioMemCompatibility, String> {
            Err("live compatibility evidence drifted: domain_xml".to_owned())
        }
    }

    const CONVERGED_XML: &str = "<domain><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></domain>";
    const PENDING_XML: &str = "<domain><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>6</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></domain>";
    const SHRINK_PENDING_XML: &str = "<domain><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>6</current></target><alias name='memory0'/></memory></domain>";
    const UNKNOWN_COMPATIBILITY_XML: &str = "<domain><memory model='virtio-mem'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></domain>";

    struct Fake {
        xml: &'static str,
        calls: Rc<RefCell<Vec<Vec<String>>>>,
    }

    impl VirshCommand for Fake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            self.calls.borrow_mut().push(arguments.to_vec());
            if arguments[0] == "dumpxml" {
                Ok(self.xml.to_owned())
            } else {
                Ok(String::new())
            }
        }
    }

    fn converged() -> VirtioMemState {
        parse_virtio_mem_xml_for_alias(CONVERGED_XML, "memory0")
            .unwrap()
            .memory
    }

    struct FailedUpdateFake {
        calls: Rc<RefCell<Vec<Vec<String>>>>,
    }

    impl VirshCommand for FailedUpdateFake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            self.calls.borrow_mut().push(arguments.to_vec());
            if arguments[0] == "dumpxml" {
                Ok(CONVERGED_XML.to_owned())
            } else {
                Err(VirshError::Timeout(std::time::Duration::from_secs(10)))
            }
        }
    }

    /// Records calls and fails every `update-memory-device` from the given
    /// (zero-based) one onwards.
    struct ScriptedFake {
        calls: Rc<RefCell<Vec<Vec<String>>>>,
        fail_updates_from: usize,
    }

    impl VirshCommand for ScriptedFake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            let updates = self
                .calls
                .borrow()
                .iter()
                .filter(|call| call[0] == "update-memory-device")
                .count();
            self.calls.borrow_mut().push(arguments.to_vec());
            match arguments[0].as_str() {
                "dumpxml" => Ok(CONVERGED_XML.to_owned()),
                "update-memory-device" if updates >= self.fail_updates_from => {
                    Err(VirshError::Timeout(std::time::Duration::from_secs(10)))
                }
                _ => Ok(String::new()),
            }
        }
    }

    fn widen_with(fail_updates_from: usize) -> (Result<(), ResizeSinkError>, Vec<String>) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = VirshResizeSink::new(
            ScriptedFake {
                calls: Rc::clone(&calls),
                fail_updates_from,
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());
        let result = sink.widen_usable_region_guarded(converged(), &AtomicBool::new(false));
        let actions = calls
            .borrow()
            .iter()
            .filter(|call| call[0] != "dumpxml")
            .map(|call| match call[0].as_str() {
                "update-memory-device" => format!("update {}", call[5]),
                operation => operation.to_owned(),
            })
            .collect();
        (result, actions)
    }

    #[test]
    fn widening_pauses_requests_the_ceiling_restores_and_resumes() {
        let (result, actions) = widen_with(usize::MAX);
        result.expect("widening succeeds");
        // An 8 GiB device: the 7 GiB ceiling, then the converged 4 GiB again.
        assert_eq!(
            actions,
            ["suspend", "update 7340032", "update 4194304", "resume"]
        );
    }

    #[test]
    fn widening_never_resumes_with_the_ceiling_requested() {
        let (result, actions) = widen_with(1);
        assert!(
            matches!(&result, Err(ResizeSinkError::CommandUnknown(message)) if message.contains("left paused")),
            "{result:?}"
        );
        assert_eq!(
            actions,
            [
                "suspend",
                "update 7340032",
                "update 4194304",
                "update 4194304"
            ]
        );

        // A failed widening command may still have applied: restore before
        // resuming, and stay paused when the restore fails too.
        let (result, actions) = widen_with(0);
        assert!(
            matches!(&result, Err(ResizeSinkError::CommandUnknown(message)) if message.contains("left paused"))
        );
        assert!(!actions.contains(&"resume".to_owned()), "{actions:?}");
    }

    #[test]
    fn refreshes_state_before_sending_one_validated_resize() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fake = Fake {
            xml: CONVERGED_XML,
            calls: Rc::clone(&calls),
        };
        let sink = VirshResizeSink::new(fake, "guest", "memory0")
            .with_external_compatibility(VirtioMemCompatibility::confirmed());

        sink.request_resize_guarded(
            converged(),
            6 * 1024 * 1024,
            &AtomicBool::new(false),
            &|| Ok(()),
        )
        .expect("aligned converged request succeeds");

        assert_eq!(
            calls.take(),
            vec![
                vec!["dumpxml", "guest"],
                vec![
                    "update-memory-device",
                    "guest",
                    "--alias",
                    "memory0",
                    "--requested-size",
                    "6144",
                    "--live",
                ],
            ]
            .into_iter()
            .map(|arguments| arguments.into_iter().map(str::to_owned).collect())
            .collect::<Vec<Vec<String>>>(),
        );
    }

    #[test]
    fn final_guard_rejects_drift_and_cancellation_during_compatibility() {
        struct Cancelling<'a>(&'a AtomicBool);
        impl CompatibilitySource for Cancelling<'_> {
            fn compatibility(&self) -> Result<VirtioMemCompatibility, String> {
                self.0.store(true, Ordering::Release);
                Ok(VirtioMemCompatibility::confirmed())
            }
        }
        let expected = parse_virtio_mem_xml_for_alias(CONVERGED_XML, "memory0")
            .unwrap()
            .memory;
        let stop = AtomicBool::new(false);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = VirshResizeSink::new(
            Fake {
                xml: CONVERGED_XML,
                calls: calls.clone(),
            },
            "guest",
            "memory0",
        )
        .with_compatibility_source(Cancelling(&stop));
        assert!(matches!(
            sink.request_resize_guarded(expected, 5 << 30, &stop, &|| Ok(())),
            Err(ResizeSinkError::Rejected(_))
        ));
        assert_eq!(calls.borrow().len(), 1);
        stop.store(false, Ordering::Release);
        let sink = VirshResizeSink::new(
            Fake {
                xml: CONVERGED_XML,
                calls: calls.clone(),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());
        let drifted = VirtioMemState {
            current_bytes: 3 << 30,
            requested_bytes: 3 << 30,
            ..expected
        };
        assert!(matches!(
            sink.request_resize_guarded(drifted, 5 << 30, &stop, &|| Ok(())),
            Err(ResizeSinkError::Rejected(_))
        ));
        assert!(calls.borrow().iter().all(|call| call[0] == "dumpxml"));
        assert!(matches!(
            sink.request_resize_guarded(expected, 5 << 30, &stop, &|| Err(
                "headroom changed".to_owned()
            )),
            Err(ResizeSinkError::Rejected(_))
        ));
    }

    #[test]
    fn rejects_pending_state_without_sending_a_resize() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fake = Fake {
            xml: PENDING_XML,
            calls: Rc::clone(&calls),
        };
        let sink = VirshResizeSink::new(fake, "guest", "memory0")
            .with_external_compatibility(VirtioMemCompatibility::confirmed());

        assert!(sink.prepare_resize(6 * 1024 * 1024).is_err());
        assert_eq!(calls.take().len(), 1);
    }

    #[test]
    fn distinguishes_preflight_rejection_from_an_unknown_command_outcome() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = VirshResizeSink::new(
            FailedUpdateFake {
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());

        let error = sink
            .request_resize_guarded(
                converged(),
                6 * 1024 * 1024,
                &AtomicBool::new(false),
                &|| Ok(()),
            )
            .expect_err("timed-out update has an unknown outcome");

        assert!(matches!(error, ResizeSinkError::CommandUnknown(_)));
        assert_eq!(calls.borrow().len(), 2);
    }

    #[test]
    fn a_stalled_shrink_settles_only_at_its_unchanged_current_allocation() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = VirshResizeSink::new(
            Fake {
                xml: SHRINK_PENDING_XML,
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());
        let pending = parse_virtio_mem_xml_for_alias(SHRINK_PENDING_XML, "memory0")
            .unwrap()
            .memory;
        let stop = AtomicBool::new(false);

        assert!(sink
            .settle_pending_guarded(
                VirtioMemState {
                    current_bytes: 5 << 30,
                    ..pending
                },
                &stop
            )
            .is_err());
        sink.settle_pending_guarded(pending, &stop)
            .expect("an unchanged pending shrink settles at current");
        let calls = calls.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[2][5], "6291456");
    }

    #[test]
    fn abandon_to_current_requires_the_qualified_pending_state() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = VirshResizeSink::new(
            Fake {
                xml: SHRINK_PENDING_XML,
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());

        let prepared = sink
            .prepare_abandon_to_current(
                4 * 1024 * 1024 * 1024,
                parse_virtio_mem_xml_for_alias(SHRINK_PENDING_XML, "memory0")
                    .unwrap()
                    .memory,
            )
            .expect("unchanged qualified state may recover to current");
        assert_eq!(prepared.current_bytes(), prepared.target_bytes());
        assert_eq!(calls.borrow().len(), 1);

        assert!(sink
            .prepare_abandon_to_current(
                4 * 1024 * 1024 * 1024,
                VirtioMemState {
                    current_bytes: 5 * 1024 * 1024 * 1024,
                    ..parse_virtio_mem_xml_for_alias(SHRINK_PENDING_XML, "memory0")
                        .unwrap()
                        .memory
                }
            )
            .is_err());
        assert_eq!(calls.borrow().len(), 2);
    }

    #[test]
    fn rejects_unknown_compatibility_before_sending_a_resize() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fake = Fake {
            xml: UNKNOWN_COMPATIBILITY_XML,
            calls: Rc::clone(&calls),
        };
        let sink = VirshResizeSink::new(fake, "guest", "memory0");

        let error = sink
            .prepare_resize(6 * 1024 * 1024)
            .map_err(ResizeSinkError::Rejected)
            .expect_err("unknown compatibility must fail closed");
        assert!(
            matches!(error, ResizeSinkError::Rejected(message) if message.contains("dynamic-memslots"))
        );
        assert_eq!(calls.take().len(), 1);
    }

    #[test]
    fn rejects_attestation_drift_before_sending_a_resize() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = VirshResizeSink::new(
            Fake {
                xml: CONVERGED_XML,
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_compatibility_source(DriftedAttestation);

        let error = sink
            .prepare_resize(6 * 1024 * 1024)
            .map_err(ResizeSinkError::Rejected)
            .expect_err("drift must fail before actuation");
        assert!(
            matches!(error, ResizeSinkError::Rejected(message) if message.contains("drifted: domain_xml"))
        );
        assert_eq!(calls.take(), Vec::<Vec<String>>::new());
    }
}
