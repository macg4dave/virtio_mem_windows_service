//! Guest-owned deltas and host responses. File reads are delivery, never an
//! acknowledgement. A guest durably retains one request (including its session
//! and expiry) until a correlated terminal result is known, even across restart.
//! A host persists admission before executing and persists its result before
//! publishing. A crash with admitted intent is uncertain, never a replay grant.
use serde::{Deserialize, Serialize};

use crate::VirtioMemState;

pub const MEMORY_REQUEST_VERSION: u16 = 1;
pub const MAX_MEMORY_REQUEST_BYTES: usize = 64 * 1024;
/// Admission freshness. Execution/result resolution may outlive this interval.
pub const MAX_REQUEST_LIFETIME_MILLIS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestIdentity {
    pub vm_name: String,
    pub service_name: String,
    pub session_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOperation {
    Add,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequest {
    pub version: u16,
    pub identity: RequestIdentity,
    pub operation: MemoryOperation,
    pub bytes: u64,
    pub issued_unix_millis: u64,
    pub expires_unix_millis: u64,
}

pub fn parse_memory_request(bytes: &[u8]) -> Result<MemoryRequest, String> {
    if bytes.len() > MAX_MEMORY_REQUEST_BYTES {
        return Err("memory request exceeds 64 KiB".to_owned());
    }
    let request: MemoryRequest =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    request.validate()?;
    Ok(request)
}

impl MemoryRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != MEMORY_REQUEST_VERSION {
            return Err("unsupported memory request version".to_owned());
        }
        for value in [
            &self.identity.vm_name,
            &self.identity.service_name,
            &self.identity.session_id,
        ] {
            if value.trim().is_empty()
                || value.len() > 128
                || !value.is_ascii()
                || value.chars().any(char::is_control)
            {
                return Err("invalid memory request identity".to_owned());
            }
        }
        if self.bytes == 0
            || self.issued_unix_millis == 0
            || self.expires_unix_millis <= self.issued_unix_millis
            || self
                .expires_unix_millis
                .saturating_sub(self.issued_unix_millis)
                > MAX_REQUEST_LIFETIME_MILLIS
        {
            return Err(
                "memory request needs a positive delta and a bounded validity interval".to_owned(),
            );
        }
        Ok(())
    }

    /// Check again at the effect boundary, after any blocking preparation.
    /// Expiry blocks a new effect; it does not erase an already admitted one.
    pub fn validate_at(
        &self,
        vm: &str,
        service: &str,
        now: u64,
        future_tolerance_millis: u64,
    ) -> Result<(), String> {
        self.validate()?;
        if self.identity.vm_name != vm || self.identity.service_name != service {
            return Err("foreign memory request".to_owned());
        }
        if self.issued_unix_millis > now.saturating_add(future_tolerance_millis)
            || now >= self.expires_unix_millis
        {
            return Err("memory request is expired or from the future".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOutcome {
    Completed,
    Partial,
    PoolEmpty,
    Rejected,
    /// The command may have taken effect. This is unresolved and must not
    /// permit a fresh delta, even if the service has since changed session.
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryResult {
    pub request: MemoryRequest,
    pub outcome: MemoryOutcome,
    pub actual_bytes: u64,
    pub observed_unix_millis: u64,
    pub reason: Option<String>,
}

impl MemoryResult {
    pub fn validate_for(&self, request: &MemoryRequest) -> Result<(), String> {
        self.request.validate()?;
        if &self.request != request {
            return Err("memory result does not match the retained request".to_owned());
        }
        if self.observed_unix_millis == 0 || self.actual_bytes > request.bytes {
            return Err("memory result has invalid observation/actual bytes".to_owned());
        }
        if self.reason.as_ref().is_some_and(|reason| {
            reason.is_empty() || reason.len() > 1024 || reason.chars().any(char::is_control)
        }) {
            return Err("invalid memory result reason".to_owned());
        }
        let valid = match self.outcome {
            MemoryOutcome::Completed => self.actual_bytes == request.bytes,
            MemoryOutcome::Partial => self.actual_bytes < request.bytes,
            MemoryOutcome::PoolEmpty => {
                request.operation == MemoryOperation::Add && self.actual_bytes == 0
            }
            MemoryOutcome::Rejected => self.actual_bytes == 0 && self.reason.is_some(),
            MemoryOutcome::Uncertain => self.reason.is_some(),
        };
        if !valid {
            return Err("memory result outcome contradicts actual bytes".to_owned());
        }
        Ok(())
    }

    pub fn terminal(&self) -> bool {
        self.outcome != MemoryOutcome::Uncertain
    }
}

/// Result-file contract for the existing QGA file transport. A snapshot without a result bootstraps Windows' device geometry;
/// only a matching response can resolve its durable pending request. Snapshots
/// must be fresh before computing another delta. An old terminal result may
/// resolve a retained identity, but never supplies fresh geometry for policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostMemoryStatus {
    pub version: u16,
    pub vm_name: String,
    pub service_name: String,
    pub device_alias: String,
    pub observed_unix_millis: u64,
    pub device_size_bytes: u64,
    pub block_size_bytes: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
    pub result: Option<MemoryResult>,
}

impl HostMemoryStatus {
    pub fn device(&self) -> VirtioMemState {
        VirtioMemState {
            size_bytes: self.device_size_bytes,
            block_size_bytes: self.block_size_bytes,
            requested_bytes: self.requested_bytes,
            current_bytes: self.current_bytes,
        }
    }

    pub fn validate_at(
        &self,
        vm: &str,
        service: &str,
        alias: &str,
        now: u64,
        max_age_millis: u64,
    ) -> Result<(), String> {
        if self.version != MEMORY_REQUEST_VERSION
            || self.vm_name != vm
            || self.service_name != service
            || self.device_alias != alias
        {
            return Err("foreign or unsupported host memory status".to_owned());
        }
        if self.observed_unix_millis == 0
            || max_age_millis == 0
            || self.observed_unix_millis > now
            || now - self.observed_unix_millis > max_age_millis
        {
            return Err("host memory status is stale or from the future".to_owned());
        }
        self.device()
            .validate()
            .map_err(|error| error.to_string())?;
        if let Some(result) = &self.result {
            result.validate_for(&result.request)?;
            if !result.request.bytes.is_multiple_of(self.block_size_bytes)
                || !result.actual_bytes.is_multiple_of(self.block_size_bytes)
            {
                return Err("memory result is not block aligned".to_owned());
            }
            if result.request.identity.vm_name != vm
                || result.request.identity.service_name != service
                || result.observed_unix_millis > self.observed_unix_millis
            {
                return Err("host memory result identity/time contradicts status".to_owned());
            }
        }
        Ok(())
    }
}

/// The retained host admission record. Persist every successful transition
/// before exposing it to transport or executing a device command. Keep the
/// terminal record as a replay watermark when admitting its successor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequestJournal {
    pub request: Option<MemoryRequest>,
    pub result: Option<MemoryResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAdmission {
    New,
    Replay,
}

impl MemoryRequestJournal {
    pub fn validate(&self) -> Result<(), String> {
        match (&self.request, &self.result) {
            (None, None) => Ok(()),
            (Some(request), result) => {
                request.validate()?;
                if let Some(result) = result {
                    result.validate_for(request)?;
                }
                Ok(())
            }
            (None, Some(_)) => Err("request journal has a result without an admission".to_owned()),
        }
    }

    pub fn admit(
        &mut self,
        request: MemoryRequest,
        vm: &str,
        service: &str,
        now: u64,
        future_tolerance_millis: u64,
    ) -> Result<RequestAdmission, String> {
        self.validate()?;
        request.validate()?;
        if request.identity.vm_name != vm || request.identity.service_name != service {
            return Err("foreign memory request".to_owned());
        }
        if let Some(previous) = &self.request {
            if previous.identity.vm_name != vm || previous.identity.service_name != service {
                return Err("foreign request journal".to_owned());
            }
            if previous.identity == request.identity {
                if previous != &request {
                    return Err("request identity was reused with different content".to_owned());
                }
                return Ok(RequestAdmission::Replay);
            }
            if self.result.as_ref().is_none_or(|result| !result.terminal()) {
                return Err(
                    "an earlier request is unresolved, including across producer sessions"
                        .to_owned(),
                );
            }
            // Issuance timestamps are a durable watermark, not just freshness.
            // The guest must wait for an advancing wall clock after a rollback.
            if request.issued_unix_millis <= previous.issued_unix_millis
                || (request.identity.session_id == previous.identity.session_id
                    && request.identity.sequence <= previous.identity.sequence)
            {
                return Err("memory request predates the retained replay watermark".to_owned());
            }
        }
        request.validate_at(vm, service, now, future_tolerance_millis)?;
        self.request = Some(request);
        self.result = None;
        Ok(RequestAdmission::New)
    }

    pub fn record_result(&mut self, result: MemoryResult) -> Result<(), String> {
        self.validate()?;
        let request = self
            .request
            .as_ref()
            .ok_or("memory result has no admitted request")?;
        result.validate_for(request)?;
        if let Some(previous) = &self.result {
            if previous.terminal() && previous != &result {
                return Err("terminal memory result cannot be replaced".to_owned());
            }
            if result.observed_unix_millis < previous.observed_unix_millis {
                return Err("memory result observation moved backwards".to_owned());
            }
        }
        self.result = Some(result);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> MemoryRequest {
        MemoryRequest {
            version: MEMORY_REQUEST_VERSION,
            identity: RequestIdentity {
                vm_name: "guest".to_owned(),
                service_name: "service".to_owned(),
                session_id: "session".to_owned(),
                sequence: 12,
            },
            operation: MemoryOperation::Add,
            bytes: 1024,
            issued_unix_millis: 100,
            expires_unix_millis: 200,
        }
    }
    #[test]
    fn expiry_and_identity_are_effect_preconditions() {
        let request = request();
        assert!(request.validate_at("guest", "service", 100, 0).is_ok());
        assert!(request.validate_at("guest", "service", 200, 0).is_err());
        assert!(request.validate_at("other", "service", 100, 0).is_err());
        assert!(request.validate_at("guest", "service", 99, 0).is_err());
        assert_eq!(
            serde_json::from_slice::<MemoryRequest>(&serde_json::to_vec(&request).unwrap())
                .unwrap(),
            request
        );
    }
    #[test]
    fn results_correlate_the_complete_request_and_report_actual_bytes() {
        let request = request();
        let mut result = MemoryResult {
            request: request.clone(),
            outcome: MemoryOutcome::Partial,
            actual_bytes: 512,
            observed_unix_millis: 150,
            reason: None,
        };
        result.validate_for(&request).unwrap();
        let mut altered = request.clone();
        altered.bytes += 1;
        assert!(result.validate_for(&altered).is_err());
        result.outcome = MemoryOutcome::PoolEmpty;
        assert!(result.validate_for(&request).is_err());
        result.actual_bytes = 0;
        result.validate_for(&request).unwrap();
        result.outcome = MemoryOutcome::Uncertain;
        result.reason = Some("command timed out".to_owned());
        result.validate_for(&request).unwrap();
        assert!(!result.terminal());
    }
    #[test]
    fn durable_admission_never_regrants_a_retransmission_or_an_uncertain_delta() {
        let first = request();
        let mut journal = MemoryRequestJournal::default();
        assert_eq!(
            journal
                .admit(first.clone(), "guest", "service", 100, 0)
                .unwrap(),
            RequestAdmission::New
        );
        let mut restored: MemoryRequestJournal =
            serde_json::from_slice(&serde_json::to_vec(&journal).unwrap()).unwrap();
        // Expiry does not delete the admission or authorise another effect.
        assert_eq!(
            restored
                .admit(first.clone(), "guest", "service", 500, 0)
                .unwrap(),
            RequestAdmission::Replay
        );
        let mut next = first.clone();
        next.identity.session_id = "new-session".to_owned();
        next.identity.sequence = 37;
        next.issued_unix_millis = 160;
        next.expires_unix_millis = 260;
        assert!(restored
            .admit(next.clone(), "guest", "service", 160, 0)
            .is_err());
        restored
            .record_result(MemoryResult {
                request: first.clone(),
                outcome: MemoryOutcome::Uncertain,
                actual_bytes: 0,
                observed_unix_millis: 150,
                reason: Some("connection lost".to_owned()),
            })
            .unwrap();
        assert!(restored
            .admit(next.clone(), "guest", "service", 160, 0)
            .is_err());
        restored
            .record_result(MemoryResult {
                request: first.clone(),
                outcome: MemoryOutcome::Partial,
                actual_bytes: 512,
                observed_unix_millis: 155,
                reason: None,
            })
            .unwrap();
        assert_eq!(
            restored.admit(next, "guest", "service", 160, 0).unwrap(),
            RequestAdmission::New
        );
        assert!(restored.admit(first, "guest", "service", 160, 0).is_err());
    }
}
