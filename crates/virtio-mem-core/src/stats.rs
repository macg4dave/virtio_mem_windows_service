use crate::error::MemoryStatsError;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GuestMemoryStat {
    stat: String,
    value: u64,
}
#[derive(Debug, Deserialize)]
struct GuestMemoryStatsResponse {
    #[serde(rename = "return")]
    stats: Option<Vec<GuestMemoryStat>>,
    id: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MemoryStats {
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
}

pub fn parse_memory_stats(response: &str) -> Result<MemoryStats, MemoryStatsError> {
    parse_memory_stats_with_id(response, None)
}

/// Parse memory statistics and optionally require an exact response id.
pub fn parse_memory_stats_with_id(
    response: &str,
    expected_id: Option<&str>,
) -> Result<MemoryStats, MemoryStatsError> {
    let response: GuestMemoryStatsResponse = serde_json::from_str(response)
        .map_err(|error| MemoryStatsError::InvalidJson(error.to_string()))?;
    if let Some(expected_id) = expected_id {
        if response.id.as_deref() != Some(expected_id) {
            return Err(MemoryStatsError::MismatchedResponseId);
        }
    }
    let stats = response
        .stats
        .ok_or(MemoryStatsError::InvalidEnvelope("missing return array"))?;
    let find = |name: &'static str| {
        stats
            .iter()
            .find(|entry| entry.stat == name)
            .map(|entry| entry.value)
            .ok_or(MemoryStatsError::MissingStat(name))
    };
    let free_bytes = find("stat-free")?;
    let total_bytes = find("stat-total")?;
    let available_bytes = stats
        .iter()
        .find(|entry| entry.stat == "stat-available")
        .map_or(free_bytes, |entry| entry.value);
    if free_bytes > total_bytes || available_bytes > total_bytes {
        return Err(MemoryStatsError::InconsistentValues);
    }
    Ok(MemoryStats {
        free_bytes,
        available_bytes,
        total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_validates_memory_stats() {
        let stats = parse_memory_stats(
            r#"{"return":[{"stat":"stat-free","value":100},{"stat":"stat-total","value":200}]}"#,
        )
        .expect("valid response");
        assert_eq!(stats.available_bytes, 100);
        assert_eq!(
            parse_memory_stats(
                r#"{"return":[{"stat":"stat-free","value":201},{"stat":"stat-total","value":200}]}"#
            ),
            Err(MemoryStatsError::InconsistentValues)
        );
    }

    #[test]
    fn validates_captured_envelopes_and_correlation_ids() {
        assert_eq!(
            parse_memory_stats_with_id(
                r#"{"return":[{"stat":"stat-free","value":100},{"stat":"stat-total","value":200}],"id":"memory-stats-1"}"#,
                Some("memory-stats-1")
            )
            .expect("matching response id"),
            MemoryStats {
                free_bytes: 100,
                available_bytes: 100,
                total_bytes: 200,
            }
        );
        assert_eq!(
            parse_memory_stats_with_id(
                r#"{"return":[],"id":"other-request"}"#,
                Some("memory-stats-1")
            ),
            Err(MemoryStatsError::MismatchedResponseId)
        );
        assert_eq!(
            parse_memory_stats(r#"{"id":"memory-stats-1"}"#),
            Err(MemoryStatsError::InvalidEnvelope("missing return array"))
        );
        assert!(matches!(
            parse_memory_stats(r#"{"error":{"class":"CommandNotFound"}}"#),
            Err(MemoryStatsError::InvalidEnvelope(_))
        ));
    }
}
