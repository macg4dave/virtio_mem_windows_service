use virtio_mem_core::parse_virtio_mem_xml_for_alias;

use crate::runtime::ResizeSink;
use crate::virsh::VirshCommand;

pub struct VirshResizeSink<C> {
    command: C,
    vm_name: String,
    alias: String,
}

impl<C> VirshResizeSink<C> {
    pub fn new(command: C, vm_name: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
            alias: alias.into(),
        }
    }
}

impl<C: VirshCommand> ResizeSink for VirshResizeSink<C> {
    fn request_resize(&self, requested_bytes: u64) -> Result<(), String> {
        let snapshot = self
            .command
            .run(&[
                "dumpxml".to_owned(),
                "--live".to_owned(),
                self.vm_name.clone(),
            ])
            .map_err(|error| error.to_string())?;
        let state = parse_virtio_mem_xml_for_alias(&snapshot, &self.alias)
            .map_err(|error| error.to_string())?
            .memory;
        state
            .validate_target(requested_bytes)
            .map_err(|error| error.to_string())?;
        if state.requested_bytes != state.current_bytes {
            return Err("refusing resize while the previous request has not converged".to_owned());
        }
        self.command
            .run(&[
                "update-memory-device".to_owned(),
                self.vm_name.clone(),
                "--alias".to_owned(),
                self.alias.clone(),
                "--requested-size".to_owned(),
                requested_bytes.to_string(),
                "--live".to_owned(),
            ])
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::virsh::{VirshCommand, VirshError};

    const CONVERGED_XML: &str = "<domain><memory model='virtio-mem'><target><size unit='MiB'>8</size><block unit='MiB'>2</block><requested unit='MiB'>4</requested><current unit='MiB'>4</current></target><alias name='memory0'/></memory></domain>";
    const PENDING_XML: &str = "<domain><memory model='virtio-mem'><target><size unit='MiB'>8</size><block unit='MiB'>2</block><requested unit='MiB'>6</requested><current unit='MiB'>4</current></target><alias name='memory0'/></memory></domain>";

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
        let sink = VirshResizeSink::new(fake, "guest", "memory0");

        sink.request_resize(6 * 1024 * 1024)
            .expect("aligned converged request succeeds");

        assert_eq!(
            calls.take(),
            vec![
                vec!["dumpxml", "--live", "guest"],
                vec![
                    "update-memory-device",
                    "guest",
                    "--alias",
                    "memory0",
                    "--requested-size",
                    "6291456",
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
        let sink = VirshResizeSink::new(fake, "guest", "memory0");

        assert!(sink.request_resize(6 * 1024 * 1024).is_err());
        assert_eq!(calls.take().len(), 1);
    }
}
