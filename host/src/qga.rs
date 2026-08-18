use virtio_mem_core::{parse_memory_stats, MemoryStats};

use crate::runtime::GuestStatsSource;
use crate::virsh::VirshCommand;

const GET_MEMORY_STATS_REQUEST: &str = r#"{"execute":"guest-get-memory-stats"}"#;

/// Memory-stat source backed by the QEMU Guest Agent `guest-get-memory-stats`
/// command. Requires a guest agent that implements that command; see
/// [`crate::dommemstat`] for a fallback that does not depend on it.
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
    fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        let response = self
            .command
            .run(&[
                "qemu-agent-command".to_owned(),
                self.vm_name.clone(),
                GET_MEMORY_STATS_REQUEST.to_owned(),
            ])
            .map_err(|error| error.to_string())?;
        parse_memory_stats(&response).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virsh::VirshError;
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
            Ok(r#"{"return":[{"stat":"stat-free","value":100},{"stat":"stat-total","value":200}]}"#.to_owned())
        }
    }
    #[test]
    fn uses_qga_memory_stats_command() {
        let stats = VirshGuestAgent::new(Fake, "guest")
            .get_memory_stats()
            .expect("command succeeds");
        assert_eq!(stats.total_bytes, 200);
    }
}
