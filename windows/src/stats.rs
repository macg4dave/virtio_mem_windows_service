use serde::Deserialize;

use crate::error::MemoryStatsError;

#[derive(Debug, Deserialize)]
struct GuestMemoryStat {
    stat: String,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct GuestMemoryStatsResponse {
    #[serde(rename = "return")]
    stats: Vec<GuestMemoryStat>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MemoryStats {
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
}

pub fn parse_memory_stats(response: &str) -> Result<MemoryStats, MemoryStatsError> {
    let response: GuestMemoryStatsResponse = serde_json::from_str(response)
        .map_err(|error| MemoryStatsError::InvalidJson(error.to_string()))?;

    let find = |name: &'static str| {
        response
            .stats
            .iter()
            .find(|entry| entry.stat == name)
            .map(|entry| entry.value)
            .ok_or(MemoryStatsError::MissingStat(name))
    };

    let free_bytes = find("stat-free")?;
    let total_bytes = find("stat-total")?;
    let available_bytes = response
        .stats
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
    fn parses_expected_memory_stats() {
        let stats = parse_memory_stats(
            r#"{"return":[
                {"stat":"stat-free","value":2147483648},
                {"stat":"stat-total","value":8589934592},
                {"stat":"stat-available","value":3221225472}
            ]}"#,
        )
        .expect("valid response should parse");

        assert_eq!(
            stats,
            MemoryStats {
                free_bytes: 2_147_483_648,
                available_bytes: 3_221_225_472,
                total_bytes: 8_589_934_592,
            }
        );
    }

    #[test]
    fn falls_back_to_free_when_available_is_missing() {
        let stats = parse_memory_stats(
            r#"{"return":[
                {"stat":"stat-free","value":100},
                {"stat":"stat-total","value":200}
            ]}"#,
        )
        .expect("response without available should parse");

        assert_eq!(stats.available_bytes, stats.free_bytes);
    }

    #[test]
    fn rejects_missing_required_stat() {
        let error = parse_memory_stats(r#"{"return":[{"stat":"stat-free","value":100}]}"#)
            .expect_err("missing total should fail");

        assert_eq!(error, MemoryStatsError::MissingStat("stat-total"));
    }

    #[test]
    fn rejects_inconsistent_values() {
        let error = parse_memory_stats(
            r#"{"return":[
                {"stat":"stat-free","value":201},
                {"stat":"stat-total","value":200}
            ]}"#,
        )
        .expect_err("free greater than total should fail");

        assert_eq!(error, MemoryStatsError::InconsistentValues);
    }
}
