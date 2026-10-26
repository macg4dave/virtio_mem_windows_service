use virtio_mem_core::VirtioMemCompatibility;

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
