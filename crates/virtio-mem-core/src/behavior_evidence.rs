use crate::VirtioMemState;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const BEHAVIOR_EVIDENCE_VERSION: u32 = 1;
const MAX_SAMPLES: usize = 10_000;
const MAX_ID_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceUnit {
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostEvidencePhase {
    Before,
    Observation,
    Recovery,
    After,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceIdentity {
    pub operation_id: String,
    pub vm_name: String,
    pub device_alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSample {
    pub identity: EvidenceIdentity,
    pub sequence: u64,
    pub wall_clock_unix_millis: u64,
    pub monotonic_millis: u64,
    pub source_id: String,
    pub unit: EvidenceUnit,
    #[serde(flatten)]
    pub evidence: EvidenceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "layer", rename_all = "snake_case")]
pub enum EvidenceKind {
    HostLibvirt {
        phase: HostEvidencePhase,
        size_bytes: u64,
        block_size_bytes: u64,
        requested_bytes: u64,
        current_bytes: u64,
    },
    WindowsHealth {
        viomem_running: bool,
    },
    ControllerState {
        enabled: bool,
        active: bool,
    },
    DriverTrace {
        requested_size_bytes: u64,
        plugged_size_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorEvidenceDocument {
    pub version: u32,
    pub identity: EvidenceIdentity,
    pub samples: Vec<EvidenceSample>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BehaviorEvidenceError {
    #[error("invalid behavior-evidence JSON: {0}")]
    InvalidJson(String),
    #[error("unsupported behavior-evidence version {0}")]
    UnsupportedVersion(u32),
    #[error("behavior evidence contains too many samples: {0}")]
    TooManySamples(usize),
    #[error("behavior evidence has an invalid {field}: {value}")]
    InvalidIdentity { field: &'static str, value: String },
    #[error("behavior evidence sample {sequence} has an invalid source id")]
    InvalidSourceId { sequence: u64 },
    #[error("behavior evidence sample {sequence} belongs to a different operation or target")]
    MixedIdentity { sequence: u64 },
    #[error("behavior evidence sequence is not strictly increasing at {sequence}")]
    NonIncreasingSequence { sequence: u64 },
    #[error("behavior evidence wall clock moves backwards at sample {sequence}")]
    WallClockMovedBackwards { sequence: u64 },
    #[error("behavior evidence monotonic clock is not strictly increasing at sample {sequence}")]
    NonIncreasingMonotonicClock { sequence: u64 },
    #[error("behavior evidence sample {sequence} has an invalid host state: {reason}")]
    InvalidHostState { sequence: u64, reason: String },
    #[error("behavior evidence host geometry changes at sample {sequence}")]
    ChangedHostGeometry { sequence: u64 },
    #[error("behavior evidence contains more than one {0:?} host endpoint")]
    DuplicateHostEndpoint(HostEvidencePhase),
    #[error("behavior evidence is missing a converged {0:?} host endpoint")]
    MissingConvergedHostEndpoint(HostEvidencePhase),
    #[error("behavior evidence places the final host endpoint before the initial endpoint")]
    ReversedHostEndpoints,
    #[error("behavior evidence is missing required {0} evidence")]
    MissingRequiredLayer(&'static str),
    #[error("behavior evidence sample {sequence} has invalid driver diagnostic values")]
    InvalidDriverTrace { sequence: u64 },
}

pub fn parse_behavior_evidence(
    json: &str,
) -> Result<BehaviorEvidenceDocument, BehaviorEvidenceError> {
    let document: BehaviorEvidenceDocument = serde_json::from_str(json)
        .map_err(|error| BehaviorEvidenceError::InvalidJson(error.to_string()))?;
    document.validate()?;
    Ok(document)
}

impl BehaviorEvidenceDocument {
    pub fn validate(&self) -> Result<(), BehaviorEvidenceError> {
        if self.version != BEHAVIOR_EVIDENCE_VERSION {
            return Err(BehaviorEvidenceError::UnsupportedVersion(self.version));
        }
        validate_identity(&self.identity)?;
        if self.samples.len() > MAX_SAMPLES {
            return Err(BehaviorEvidenceError::TooManySamples(self.samples.len()));
        }

        let mut previous_sequence = None;
        let mut previous_wall_clock = None;
        let mut previous_monotonic = None;
        let mut geometry = None;
        let mut before_index = None;
        let mut after_index = None;
        let mut saw_windows_health = false;
        let mut saw_controller_state = false;
        let mut driver_samples = Vec::new();

        for (index, sample) in self.samples.iter().enumerate() {
            if sample.identity != self.identity {
                return Err(BehaviorEvidenceError::MixedIdentity {
                    sequence: sample.sequence,
                });
            }
            validate_source_id(sample)?;
            if previous_sequence.is_some_and(|previous| sample.sequence <= previous) {
                return Err(BehaviorEvidenceError::NonIncreasingSequence {
                    sequence: sample.sequence,
                });
            }
            if previous_wall_clock.is_some_and(|previous| sample.wall_clock_unix_millis < previous)
            {
                return Err(BehaviorEvidenceError::WallClockMovedBackwards {
                    sequence: sample.sequence,
                });
            }
            if previous_monotonic.is_some_and(|previous| sample.monotonic_millis <= previous) {
                return Err(BehaviorEvidenceError::NonIncreasingMonotonicClock {
                    sequence: sample.sequence,
                });
            }
            previous_sequence = Some(sample.sequence);
            previous_wall_clock = Some(sample.wall_clock_unix_millis);
            previous_monotonic = Some(sample.monotonic_millis);

            match sample.evidence {
                EvidenceKind::HostLibvirt {
                    phase,
                    size_bytes,
                    block_size_bytes,
                    requested_bytes,
                    current_bytes,
                } => {
                    let state = VirtioMemState {
                        size_bytes,
                        block_size_bytes,
                        requested_bytes,
                        current_bytes,
                    };
                    state
                        .validate()
                        .map_err(|error| BehaviorEvidenceError::InvalidHostState {
                            sequence: sample.sequence,
                            reason: error.to_string(),
                        })?;
                    let sample_geometry = (size_bytes, block_size_bytes);
                    if geometry.is_some_and(|expected| expected != sample_geometry) {
                        return Err(BehaviorEvidenceError::ChangedHostGeometry {
                            sequence: sample.sequence,
                        });
                    }
                    geometry = Some(sample_geometry);
                    match phase {
                        HostEvidencePhase::Before => {
                            if before_index.replace(index).is_some() {
                                return Err(BehaviorEvidenceError::DuplicateHostEndpoint(phase));
                            }
                            if requested_bytes != current_bytes {
                                return Err(BehaviorEvidenceError::MissingConvergedHostEndpoint(
                                    phase,
                                ));
                            }
                        }
                        HostEvidencePhase::After => {
                            if after_index.replace(index).is_some() {
                                return Err(BehaviorEvidenceError::DuplicateHostEndpoint(phase));
                            }
                            if requested_bytes != current_bytes {
                                return Err(BehaviorEvidenceError::MissingConvergedHostEndpoint(
                                    phase,
                                ));
                            }
                        }
                        HostEvidencePhase::Observation | HostEvidencePhase::Recovery => {}
                    }
                }
                EvidenceKind::WindowsHealth { .. } => saw_windows_health = true,
                EvidenceKind::ControllerState { .. } => saw_controller_state = true,
                EvidenceKind::DriverTrace {
                    requested_size_bytes,
                    plugged_size_bytes,
                } => {
                    driver_samples.push((sample.sequence, requested_size_bytes, plugged_size_bytes))
                }
            }
        }

        let before_index = before_index.ok_or(
            BehaviorEvidenceError::MissingConvergedHostEndpoint(HostEvidencePhase::Before),
        )?;
        let after_index = after_index.ok_or(
            BehaviorEvidenceError::MissingConvergedHostEndpoint(HostEvidencePhase::After),
        )?;
        if after_index <= before_index {
            return Err(BehaviorEvidenceError::ReversedHostEndpoints);
        }
        if !saw_windows_health {
            return Err(BehaviorEvidenceError::MissingRequiredLayer(
                "Windows health",
            ));
        }
        if !saw_controller_state {
            return Err(BehaviorEvidenceError::MissingRequiredLayer(
                "controller state",
            ));
        }

        let (size_bytes, block_size_bytes) = geometry.ok_or(
            BehaviorEvidenceError::MissingRequiredLayer("host libvirt state"),
        )?;
        for (sequence, requested_size_bytes, plugged_size_bytes) in driver_samples {
            if requested_size_bytes > size_bytes
                || plugged_size_bytes > size_bytes
                || !requested_size_bytes.is_multiple_of(block_size_bytes)
                || !plugged_size_bytes.is_multiple_of(block_size_bytes)
            {
                return Err(BehaviorEvidenceError::InvalidDriverTrace { sequence });
            }
        }

        Ok(())
    }
}

fn validate_identity(identity: &EvidenceIdentity) -> Result<(), BehaviorEvidenceError> {
    validate_id("operation_id", &identity.operation_id)?;
    validate_id("vm_name", &identity.vm_name)?;
    validate_id("device_alias", &identity.device_alias)
}

fn validate_id(field: &'static str, value: &str) -> Result<(), BehaviorEvidenceError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(BehaviorEvidenceError::InvalidIdentity {
            field,
            value: value.to_owned(),
        })
    }
}

fn validate_source_id(sample: &EvidenceSample) -> Result<(), BehaviorEvidenceError> {
    if sample.source_id.is_empty()
        || sample.source_id.len() > MAX_ID_LENGTH
        || sample.source_id.chars().any(char::is_control)
    {
        Err(BehaviorEvidenceError::InvalidSourceId {
            sequence: sample.sequence,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;
    const BLOCK: u64 = 2 << 20;

    fn identity() -> EvidenceIdentity {
        EvidenceIdentity {
            operation_id: "operation-1".to_owned(),
            vm_name: "guest".to_owned(),
            device_alias: "memory0".to_owned(),
        }
    }

    fn sample(sequence: u64, evidence: EvidenceKind) -> EvidenceSample {
        EvidenceSample {
            identity: identity(),
            sequence,
            wall_clock_unix_millis: 1_788_633_000_000 + sequence,
            monotonic_millis: sequence * 10,
            source_id: match evidence {
                EvidenceKind::HostLibvirt { .. } => "libvirt:qemu:///system".to_owned(),
                EvidenceKind::WindowsHealth { .. } => "windows-scm:guest".to_owned(),
                EvidenceKind::ControllerState { .. } => "systemd:rhel-host".to_owned(),
                EvidenceKind::DriverTrace { .. } => "driver:guest".to_owned(),
            },
            unit: EvidenceUnit::Bytes,
            evidence,
        }
    }

    fn host(phase: HostEvidencePhase, requested_bytes: u64, current_bytes: u64) -> EvidenceKind {
        EvidenceKind::HostLibvirt {
            phase,
            size_bytes: 20 * GIB,
            block_size_bytes: BLOCK,
            requested_bytes,
            current_bytes,
        }
    }

    fn valid_document() -> BehaviorEvidenceDocument {
        BehaviorEvidenceDocument {
            version: BEHAVIOR_EVIDENCE_VERSION,
            identity: identity(),
            samples: vec![
                sample(1, host(HostEvidencePhase::Before, GIB, GIB)),
                sample(
                    2,
                    EvidenceKind::ControllerState {
                        enabled: true,
                        active: false,
                    },
                ),
                sample(
                    3,
                    EvidenceKind::WindowsHealth {
                        viomem_running: true,
                    },
                ),
                sample(4, host(HostEvidencePhase::Observation, GIB + BLOCK, GIB)),
                sample(
                    5,
                    EvidenceKind::DriverTrace {
                        requested_size_bytes: GIB + BLOCK,
                        plugged_size_bytes: GIB + BLOCK,
                    },
                ),
                sample(6, host(HostEvidencePhase::After, GIB + BLOCK, GIB + BLOCK)),
            ],
        }
    }

    #[test]
    fn accepts_correlated_evidence_with_or_without_driver_trace() {
        let with_trace = valid_document();
        assert_eq!(with_trace.validate(), Ok(()));

        let without_trace = BehaviorEvidenceDocument {
            samples: with_trace
                .samples
                .into_iter()
                .filter(|sample| !matches!(sample.evidence, EvidenceKind::DriverTrace { .. }))
                .collect(),
            ..with_trace
        };
        assert_eq!(without_trace.validate(), Ok(()));
    }

    #[test]
    fn parses_versioned_json_and_rejects_ambiguous_units() {
        let json = serde_json::to_string(&valid_document()).expect("serialize fixture");
        assert_eq!(parse_behavior_evidence(&json), Ok(valid_document()));

        let missing_unit = json.replace("\"unit\":\"bytes\",", "");
        assert!(matches!(
            parse_behavior_evidence(&missing_unit),
            Err(BehaviorEvidenceError::InvalidJson(_))
        ));
        let wrong_unit = json.replace("\"unit\":\"bytes\"", "\"unit\":\"kib\"");
        assert!(matches!(
            parse_behavior_evidence(&wrong_unit),
            Err(BehaviorEvidenceError::InvalidJson(_))
        ));
    }

    #[test]
    fn rejects_mixed_operations_and_targets() {
        let mut document = valid_document();
        document.samples[2].identity.operation_id = "other-operation".to_owned();
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::MixedIdentity { sequence: 3 })
        );

        let mut document = valid_document();
        document.samples[2].identity.vm_name = "other-vm".to_owned();
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::MixedIdentity { sequence: 3 })
        );
    }

    #[test]
    fn rejects_non_monotonic_ordering() {
        let mut document = valid_document();
        document.samples[2].sequence = 2;
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::NonIncreasingSequence { sequence: 2 })
        );

        let mut document = valid_document();
        document.samples[2].monotonic_millis = 20;
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::NonIncreasingMonotonicClock { sequence: 3 })
        );

        let mut document = valid_document();
        document.samples[2].wall_clock_unix_millis = 1;
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::WallClockMovedBackwards { sequence: 3 })
        );
    }

    #[test]
    fn rejects_missing_required_layers() {
        let mut document = valid_document();
        document
            .samples
            .retain(|sample| !matches!(sample.evidence, EvidenceKind::WindowsHealth { .. }));
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::MissingRequiredLayer(
                "Windows health"
            ))
        );

        let mut document = valid_document();
        document
            .samples
            .retain(|sample| !matches!(sample.evidence, EvidenceKind::ControllerState { .. }));
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::MissingRequiredLayer(
                "controller state"
            ))
        );
    }

    #[test]
    fn rejects_missing_or_divergent_convergence_endpoints() {
        let mut document = valid_document();
        document.samples.retain(|sample| {
            !matches!(
                sample.evidence,
                EvidenceKind::HostLibvirt {
                    phase: HostEvidencePhase::After,
                    ..
                }
            )
        });
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::MissingConvergedHostEndpoint(
                HostEvidencePhase::After
            ))
        );

        let mut document = valid_document();
        if let EvidenceKind::HostLibvirt { current_bytes, .. } = &mut document.samples[5].evidence {
            *current_bytes = GIB;
        }
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::MissingConvergedHostEndpoint(
                HostEvidencePhase::After
            ))
        );
    }

    #[test]
    fn rejects_geometry_drift_and_invalid_driver_values() {
        let mut document = valid_document();
        if let EvidenceKind::HostLibvirt { size_bytes, .. } = &mut document.samples[3].evidence {
            *size_bytes = 18 * GIB;
        }
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::ChangedHostGeometry { sequence: 4 })
        );

        let mut document = valid_document();
        if let EvidenceKind::DriverTrace {
            plugged_size_bytes, ..
        } = &mut document.samples[4].evidence
        {
            *plugged_size_bytes += 1;
        }
        assert_eq!(
            document.validate(),
            Err(BehaviorEvidenceError::InvalidDriverTrace { sequence: 5 })
        );
    }
}
