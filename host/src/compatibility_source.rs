use serde_json::{json, Value};
use virtio_mem_core::{CompatibilityEvidence, VirtioMemCompatibility};

use crate::virsh::VirshCommand;

/// Supplies compatibility evidence immediately before resize preparation.
pub trait CompatibilitySource {
    fn compatibility(&self) -> Result<VirtioMemCompatibility, String>;
}

/// Deterministic compatibility evidence for tests and injected integrations.
#[derive(Debug, Clone, Copy)]
pub struct FixedCompatibilitySource {
    evidence: VirtioMemCompatibility,
}

impl FixedCompatibilitySource {
    /// Construct a source that always returns the supplied evidence.
    pub const fn new(evidence: VirtioMemCompatibility) -> Self {
        Self { evidence }
    }
}

impl Default for FixedCompatibilitySource {
    fn default() -> Self {
        Self::new(VirtioMemCompatibility::unknown())
    }
}

impl CompatibilitySource for FixedCompatibilitySource {
    fn compatibility(&self) -> Result<VirtioMemCompatibility, String> {
        Ok(self.evidence)
    }
}

/// Reads alias-scoped virtio-mem properties from the live QEMU QOM tree.
pub struct VirshQmpCompatibilitySource<C> {
    command: C,
    vm_name: String,
    alias: String,
    workload_review: CompatibilityEvidence,
}

impl<C> VirshQmpCompatibilitySource<C> {
    /// Construct a bounded-command compatibility source for one VM and alias.
    pub fn new(
        command: C,
        vm_name: impl Into<String>,
        alias: impl Into<String>,
        workload_review: CompatibilityEvidence,
    ) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
            alias: alias.into(),
            workload_review,
        }
    }
}

impl<C: VirshCommand> VirshQmpCompatibilitySource<C> {
    fn property(&self, property: &'static str) -> Result<CompatibilityEvidence, String> {
        let path = format!("/machine/peripheral/{}", self.alias);
        let request = json!({
            "execute": "qom-get",
            "arguments": { "path": path, "property": property }
        })
        .to_string();
        let response = self
            .command
            .run(&[
                "qemu-monitor-command".to_owned(),
                self.vm_name.clone(),
                request,
            ])
            .map_err(|error| format!("read live QMP property {property}: {error}"))?;
        let response: Value = serde_json::from_str(&response)
            .map_err(|error| format!("parse live QMP property {property}: {error}"))?;
        let value = response
            .get("return")
            .ok_or_else(|| format!("live QMP property {property} response is missing return"))?;
        match value {
            Value::Bool(true) => Ok(CompatibilityEvidence::Confirmed),
            Value::Bool(false) => Ok(CompatibilityEvidence::Rejected),
            Value::String(value) if matches!(value.as_str(), "on" | "yes" | "true" | "1") => {
                Ok(CompatibilityEvidence::Confirmed)
            }
            Value::String(value) if matches!(value.as_str(), "off" | "no" | "false" | "0") => {
                Ok(CompatibilityEvidence::Rejected)
            }
            _ => Err(format!(
                "live QMP property {property} has unsupported value {value}"
            )),
        }
    }
}

impl<C: VirshCommand> CompatibilitySource for VirshQmpCompatibilitySource<C> {
    fn compatibility(&self) -> Result<VirtioMemCompatibility, String> {
        if self.alias.is_empty()
            || !self
                .alias
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        {
            return Err("QMP compatibility alias contains unsupported characters".to_owned());
        }
        Ok(VirtioMemCompatibility {
            dynamic_memslots: self.property("dynamic-memslots")?,
            unplugged_inaccessible: self.property("unplugged-inaccessible")?,
            workload_review: self.workload_review,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;
    use crate::virsh::VirshError;

    struct FakeVirsh {
        responses: RefCell<VecDeque<Result<String, VirshError>>>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeVirsh {
        fn new(responses: impl IntoIterator<Item = Result<String, VirshError>>) -> Self {
            Self {
                responses: RefCell::new(responses.into_iter().collect()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl VirshCommand for FakeVirsh {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            self.calls.borrow_mut().push(arguments.to_vec());
            self.responses
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| VirshError::Failed {
                    status: "missing fixture".to_owned(),
                    stderr: String::new(),
                })?
        }
    }

    #[test]
    fn reads_confirmed_live_qmp_properties_for_the_selected_alias() {
        let fake = FakeVirsh::new([
            Ok(r#"{"return":true,"id":"libvirt-1"}"#.to_owned()),
            Ok(r#"{"return":"on","id":"libvirt-2"}"#.to_owned()),
        ]);
        let source = VirshQmpCompatibilitySource::new(
            fake,
            "guest",
            "memory0",
            CompatibilityEvidence::Confirmed,
        );

        assert_eq!(
            source.compatibility(),
            Ok(VirtioMemCompatibility::confirmed())
        );
        let calls = source.command.calls.borrow();
        assert_eq!(calls.len(), 2);
        for (call, property) in calls
            .iter()
            .zip(["dynamic-memslots", "unplugged-inaccessible"])
        {
            assert_eq!(call[0], "qemu-monitor-command");
            assert_eq!(call[1], "guest");
            let request: Value = serde_json::from_str(&call[2]).expect("valid request JSON");
            assert_eq!(request["execute"], "qom-get");
            assert_eq!(request["arguments"]["path"], "/machine/peripheral/memory0");
            assert_eq!(request["arguments"]["property"], property);
        }
    }

    #[test]
    fn preserves_rejected_and_malformed_qmp_evidence() {
        let rejected = VirshQmpCompatibilitySource::new(
            FakeVirsh::new([
                Ok(r#"{"return":false}"#.to_owned()),
                Ok(r#"{"return":"off"}"#.to_owned()),
            ]),
            "guest",
            "memory0",
            CompatibilityEvidence::Unknown,
        )
        .compatibility()
        .expect("recognized disabled values");
        assert_eq!(rejected.dynamic_memslots, CompatibilityEvidence::Rejected);
        assert_eq!(
            rejected.unplugged_inaccessible,
            CompatibilityEvidence::Rejected
        );

        let malformed = VirshQmpCompatibilitySource::new(
            FakeVirsh::new([Ok(r#"{"return":42}"#.to_owned())]),
            "guest",
            "memory0",
            CompatibilityEvidence::Confirmed,
        )
        .compatibility()
        .expect_err("unsupported QMP values must fail closed");
        assert!(malformed.contains("unsupported value"));
    }

    #[test]
    fn rejects_unsafe_alias_before_querying_qmp() {
        let source = VirshQmpCompatibilitySource::new(
            FakeVirsh::new([]),
            "guest",
            "../other-device",
            CompatibilityEvidence::Confirmed,
        );

        assert_eq!(
            source.compatibility(),
            Err("QMP compatibility alias contains unsupported characters".to_owned())
        );
        assert!(source.command.calls.borrow().is_empty());
    }
}
