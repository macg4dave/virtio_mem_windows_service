use virtio_mem_core::{bytes_to_kibibytes, parse_virtio_mem_xml_for_alias, VirtioMemCompatibility};

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

    pub fn with_external_compatibility(mut self, compatibility: VirtioMemCompatibility) -> Self {
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
    fn request_resize(&self, requested_bytes: u64) -> Result<(), ResizeSinkError> {
        let prepared = self
            .prepare_resize(requested_bytes)
            .map_err(ResizeSinkError::Rejected)?;
        self.apply_prepared(prepared)
            .map_err(ResizeSinkError::CommandUnknown)
    }

    fn renotify_shrink(&self, target_bytes: u64) -> Result<(), ResizeSinkError> {
        let prepared = self
            .prepare_shrink_renotification(target_bytes)
            .map_err(ResizeSinkError::Rejected)?;
        self.apply_prepared(prepared)
            .map_err(ResizeSinkError::CommandUnknown)
    }

    fn supersede_shrink(
        &self,
        prior_requested_bytes: u64,
        prior_current_bytes: u64,
        target_bytes: u64,
    ) -> Result<(), ResizeSinkError> {
        let prepared = self
            .prepare_shrink_supersession(prior_requested_bytes, prior_current_bytes, target_bytes)
            .map_err(ResizeSinkError::Rejected)?;
        self.apply_prepared(prepared)
            .map_err(ResizeSinkError::CommandUnknown)
    }
}

impl<C: VirshCommand, E: CompatibilitySource> VirshResizeSink<C, E> {
    pub(crate) fn prepare_resize(&self, requested_bytes: u64) -> Result<PreparedResize, String> {
        let snapshot = self
            .command
            .run(&["dumpxml".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let snapshot = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?;
        snapshot
            .compatibility
            .merge(self.compatibility_source.compatibility()?)
            .map_err(|error| error.to_string())?
            .validate_for_resize()
            .map_err(|error| error.to_string())?;
        let state = snapshot.memory;
        state
            .validate_target(requested_bytes)
            .map_err(|error| error.to_string())?;
        if state.requested_bytes != state.current_bytes {
            return Err("refusing resize while the previous request has not converged".to_owned());
        }
        let requested_kib = bytes_to_kibibytes(requested_bytes)
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
            target_bytes: requested_bytes,
        })
    }

    pub(crate) fn apply_prepared(&self, prepared: PreparedResize) -> Result<(), String> {
        self.command
            .run(prepared.arguments())
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn prepare_shrink_renotification(&self, target_bytes: u64) -> Result<PreparedResize, String> {
        let snapshot = self
            .command
            .run(&["dumpxml".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let snapshot = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?;
        snapshot
            .compatibility
            .merge(self.compatibility_source.compatibility()?)
            .map_err(|error| error.to_string())?
            .validate_for_resize()
            .map_err(|error| error.to_string())?;
        let state = snapshot.memory;
        state
            .validate_target(target_bytes)
            .map_err(|error| error.to_string())?;
        if state.requested_bytes != target_bytes || state.current_bytes <= target_bytes {
            return Err("shrink re-notification requires requested == target < current".to_owned());
        }
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

    fn prepare_shrink_supersession(
        &self,
        prior_requested_bytes: u64,
        prior_current_bytes: u64,
        target_bytes: u64,
    ) -> Result<PreparedResize, String> {
        let snapshot = self
            .command
            .run(&["dumpxml".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let snapshot = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?;
        snapshot
            .compatibility
            .merge(self.compatibility_source.compatibility()?)
            .map_err(|error| error.to_string())?
            .validate_for_resize()
            .map_err(|error| error.to_string())?;
        let state = snapshot.memory;
        state
            .validate_target(target_bytes)
            .map_err(|error| error.to_string())?;
        if state.requested_bytes != prior_requested_bytes
            || state.current_bytes != prior_current_bytes
            || prior_requested_bytes >= prior_current_bytes
            || target_bytes <= prior_requested_bytes
            || target_bytes > prior_current_bytes
        {
            return Err(
                "pending-shrink supersession requires unchanged requested < target <= current"
                    .to_owned(),
            );
        }
        let requested_kib = bytes_to_kibibytes(target_bytes)
            .ok_or_else(|| "supersession target must be an integer number of KiB".to_owned())?;
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

    /// Prepares the one-shot abandon-to-current recovery after two
    /// independent observations have qualified `stable_current_bytes`.
    /// This immediate read is the final race check before an operator applies
    /// the command.
    pub(crate) fn prepare_abandon_to_current(
        &self,
        immutable_target_bytes: u64,
        stable_current_bytes: u64,
    ) -> Result<PreparedResize, String> {
        let snapshot = self
            .command
            .run(&["dumpxml".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let snapshot = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?;
        snapshot
            .compatibility
            .merge(self.compatibility_source.compatibility()?)
            .map_err(|error| error.to_string())?
            .validate_for_resize()
            .map_err(|error| error.to_string())?;
        let state = snapshot.memory;
        state
            .validate_target(stable_current_bytes)
            .map_err(|error| error.to_string())?;
        if state.requested_bytes != immutable_target_bytes
            || state.current_bytes != stable_current_bytes
            || state.current_bytes <= immutable_target_bytes
        {
            return Err(
                "live state changed before abandon-to-current recovery could be applied".to_owned(),
            );
        }
        let requested_kib = bytes_to_kibibytes(stable_current_bytes)
            .ok_or_else(|| "recovery target must be an integer number of KiB".to_owned())?;
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
            target_bytes: stable_current_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::virsh::{VirshCommand, VirshError};

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

    #[test]
    fn refreshes_state_before_sending_one_validated_resize() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fake = Fake {
            xml: CONVERGED_XML,
            calls: Rc::clone(&calls),
        };
        let sink = VirshResizeSink::new(fake, "guest", "memory0")
            .with_external_compatibility(VirtioMemCompatibility::confirmed());

        sink.request_resize(6 * 1024 * 1024)
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
    fn rejects_pending_state_without_sending_a_resize() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fake = Fake {
            xml: PENDING_XML,
            calls: Rc::clone(&calls),
        };
        let sink = VirshResizeSink::new(fake, "guest", "memory0")
            .with_external_compatibility(VirtioMemCompatibility::confirmed());

        assert!(sink.request_resize(6 * 1024 * 1024).is_err());
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
            .request_resize(6 * 1024 * 1024)
            .expect_err("timed-out update has an unknown outcome");

        assert!(matches!(error, ResizeSinkError::CommandUnknown(_)));
        assert_eq!(calls.borrow().len(), 2);
    }

    #[test]
    fn renotification_uses_dedicated_exact_target_pending_precondition() {
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

        sink.renotify_shrink(4 * 1024 * 1024 * 1024)
            .expect("exact shrink target may be re-notified");
        assert_eq!(calls.borrow().len(), 2);

        let wrong = VirshResizeSink::new(
            Fake {
                xml: SHRINK_PENDING_XML,
                calls: Rc::clone(&calls),
            },
            "guest",
            "memory0",
        )
        .with_external_compatibility(VirtioMemCompatibility::confirmed());
        assert!(wrong.renotify_shrink(2 * 1024 * 1024 * 1024).is_err());
    }

    #[test]
    fn pending_shrink_may_only_be_superseded_upward_to_current() {
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

        sink.supersede_shrink(
            4 * 1024 * 1024 * 1024,
            6 * 1024 * 1024 * 1024,
            5 * 1024 * 1024 * 1024,
        )
        .expect("an unchanged pending shrink may be raised");
        assert_eq!(calls.borrow().len(), 2);

        assert!(sink
            .supersede_shrink(
                4 * 1024 * 1024 * 1024,
                6 * 1024 * 1024 * 1024,
                3 * 1024 * 1024 * 1024,
            )
            .is_err());
        assert!(sink
            .supersede_shrink(
                4 * 1024 * 1024 * 1024,
                6 * 1024 * 1024 * 1024,
                7 * 1024 * 1024 * 1024,
            )
            .is_err());
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
            .prepare_abandon_to_current(4 * 1024 * 1024 * 1024, 6 * 1024 * 1024 * 1024)
            .expect("unchanged qualified state may recover to current");
        assert_eq!(prepared.current_bytes(), prepared.target_bytes());
        assert_eq!(calls.borrow().len(), 1);

        assert!(sink
            .prepare_abandon_to_current(4 * 1024 * 1024 * 1024, 5 * 1024 * 1024 * 1024)
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
            .request_resize(6 * 1024 * 1024)
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
            .request_resize(6 * 1024 * 1024)
            .expect_err("drift must fail before actuation");
        assert!(
            matches!(error, ResizeSinkError::Rejected(message) if message.contains("drifted: domain_xml"))
        );
        assert_eq!(
            calls.take(),
            vec![vec!["dumpxml".to_owned(), "guest".to_owned()]]
        );
    }
}
