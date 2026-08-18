use crate::runtime::GuestStatsSource;
use crate::virsh::{VirshCommand, VirshError};

const GET_MEMORY_STATS_REQUEST: &str = r#"{"execute":"guest-get-memory-stats"}"#;

pub struct VirshGuestAgent<C> {
    command: C,
    vm_name: String,
}

impl<C> VirshGuestAgent<C> {
    pub fn new(command: C, vm_name: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
        }
    }
}

impl<C: VirshCommand> GuestStatsSource for VirshGuestAgent<C> {
    fn get_memory_stats(&self) -> Result<String, VirshError> {
        self.command.run(&[
            "qemu-agent-command".to_owned(),
            self.vm_name.clone(),
            GET_MEMORY_STATS_REQUEST.to_owned(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virsh::VirshCommand;
    struct Fake;
    impl VirshCommand for Fake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            assert_eq!(
                arguments,
                &[
                    "qemu-agent-command".to_owned(),
                    "guest".to_owned(),
                    GET_MEMORY_STATS_REQUEST.to_owned()
                ]
            );
            Ok("{}".to_owned())
        }
    }
    #[test]
    fn uses_qga_memory_stats_command() {
        assert_eq!(
            VirshGuestAgent::new(Fake, "guest")
                .get_memory_stats()
                .expect("command succeeds"),
            "{}"
        );
    }
}
