use virtio_mem_core::{bytes_to_kibibytes, parse_virtio_mem_xml_for_alias, VirtioMemCompatibility};

use crate::runtime::ResizeSink;
use crate::virsh::VirshCommand;

pub struct VirshResizeSink<C> {
    command: C,
    vm_name: String,
    alias: String,
    external_compatibility: VirtioMemCompatibility,
}

impl<C> VirshResizeSink<C> {
    pub fn new(command: C, vm_name: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
            alias: alias.into(),
            external_compatibility: VirtioMemCompatibility::unknown(),
        }
    }

    pub fn with_external_compatibility(mut self, compatibility: VirtioMemCompatibility) -> Self {
        self.external_compatibility = compatibility;
        self
    }
}

impl<C: VirshCommand> ResizeSink for VirshResizeSink<C> {
    fn request_resize(&self, requested_bytes: u64) -> Result<(), String> {
        let arguments = self.prepare_resize(requested_bytes)?;
        self.command
            .run(&arguments)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl<C: VirshCommand> VirshResizeSink<C> {
    pub fn prepare_resize(&self, requested_bytes: u64) -> Result<Vec<String>, String> {
        let snapshot = self
            .command
            .run(&["dumpxml".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let snapshot = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?;
        snapshot
            .compatibility
            .merge(self.external_compatibility)
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
        Ok(vec![
            "update-memory-device".to_owned(),
            self.vm_name.clone(),
            "--alias".to_owned(),
            self.alias.clone(),
            "--requested-size".to_owned(),
            requested_kib.to_string(),
            "--live".to_owned(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::virsh::{VirshCommand, VirshError};

    const CONVERGED_XML: &str = "<domain><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>4</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></domain>";
    const PENDING_XML: &str = "<domain><memory model='virtio-mem' dynamic-memslots='on' unplugged-inaccessible='on'><target><size unit='GiB'>8</size><block unit='MiB'>2</block><requested unit='GiB'>6</requested><current unit='GiB'>4</current></target><alias name='memory0'/></memory></domain>";
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
        assert!(error.contains("dynamic-memslots"));
        assert_eq!(calls.take().len(), 1);
    }
}
