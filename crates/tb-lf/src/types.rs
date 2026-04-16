use serde::ser::Serializer;
use serde::{Deserialize, Deserializer, Serialize};

/// Tri-state for nullable fields in update payloads.
/// - `Nullable::Absent` → field omitted from JSON
/// - `Nullable::Null` → field serialized as `null` (clears the value)
/// - `Nullable::Value(v)` → field serialized with the value
#[derive(Debug, Clone, Default)]
pub enum Nullable<T> {
    #[default]
    Absent,
    Null,
    Value(T),
}

impl<T> Nullable<T> {
    pub fn is_absent(&self) -> bool {
        matches!(self, Nullable::Absent)
    }
}

impl<T: Serialize> Serialize for Nullable<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Nullable::Absent => unreachable!("Absent should be skipped by skip_serializing_if"),
            Nullable::Null => serializer.serialize_none(),
            Nullable::Value(v) => v.serialize(serializer),
        }
    }
}

/// Deserialize a value that may come as a number or a string representation of a number.
fn string_or_f64<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNum {
        Num(f64),
        Str(String),
        Null,
    }
    match StringOrNum::deserialize(d)? {
        StringOrNum::Num(n) => Ok(Some(n)),
        StringOrNum::Str(s) => Ok(s.parse().ok()),
        StringOrNum::Null => Ok(None),
    }
}

fn string_or_u64<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNum {
        Num(u64),
        Str(String),
        Null,
    }
    match StringOrNum::deserialize(d)? {
        StringOrNum::Num(n) => Ok(Some(n)),
        StringOrNum::Str(s) => Ok(s.parse().ok()),
        StringOrNum::Null => Ok(None),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Trace {
    pub id: i64,
    pub langfuse_id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub cost_usd: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub user_query: Option<String>,
    #[serde(default)]
    pub agent_response: Option<String>,
    #[serde(default)]
    pub triage_status: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub user_satisfied: Option<bool>,
    #[serde(default)]
    pub user_feedback: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceStats {
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_traces: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub total_cost: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_duration_ms: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub max_duration_ms: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Session {
    pub session_id: String,
    pub trace_count: u32,
    pub first_trace_at: String,
    pub last_trace_at: String,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub total_cost_usd: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub max_latency_ms: Option<f64>,
    #[serde(default)]
    pub user_ids: Option<Vec<String>>,
    #[serde(default)]
    pub environments: Option<Vec<String>>,
    #[serde(default)]
    pub user_satisfied: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Score {
    pub id: i64,
    pub langfuse_id: String,
    pub trace_langfuse_id: String,
    pub name: String,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub value: Option<f64>,
    #[serde(default)]
    pub string_value: Option<String>,
    #[serde(default)]
    pub data_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub environment: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Observation {
    pub id: i64,
    pub langfuse_id: String,
    pub trace_langfuse_id: String,
    #[serde(default)]
    pub observation_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DailyMetric {
    pub id: i64,
    pub date: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub trace_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub unique_users: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub total_cost_usd: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_latency_ms: Option<f64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub eval_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub eval_avg_score: Option<f64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub error_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub environment: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Comment {
    pub id: i64,
    #[serde(default)]
    pub trace_langfuse_id: Option<String>,
    #[serde(default)]
    pub object_type: Option<String>,
    #[serde(default)]
    pub object_id: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

// Dashboard is a complex nested type — use Value for now
pub type Dashboard = serde_json::Value;

// --- Triage queue ---

#[derive(Debug, Deserialize, Serialize)]
pub struct QueueItem {
    pub id: i64,
    #[serde(default)]
    pub trace_langfuse_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub ai_category: Option<String>,
    #[serde(default)]
    pub ai_confidence: Option<String>,
    #[serde(default)]
    pub ai_reasoning: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub reviewed_by: Option<String>,
    #[serde(default)]
    pub feature_id: Option<i64>,
    #[serde(default)]
    pub created_at: Option<String>,
    // --- new fields ---
    #[serde(default)]
    pub ai_team: Option<NamedRef>,
    #[serde(default)]
    pub team: Option<NamedRef>,
    #[serde(default)]
    pub ai_feature: Option<NamedRef>,
    #[serde(default)]
    pub feature: Option<NamedRef>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
    #[serde(default)]
    pub triage_run_id: Option<i64>,
    #[serde(default)]
    pub trace_timestamp: Option<String>,
    #[serde(default)]
    pub trace_user_id: Option<String>,
    #[serde(default)]
    pub trace_user_satisfied: Option<bool>,
}

/// Payload for PATCH /queue_items/:id and PATCH /queue_items/bulk_update
#[derive(Debug, Default, Serialize)]
pub struct QueueItemUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Nullable::is_absent")]
    pub feature_id: Nullable<i64>,
    #[serde(skip_serializing_if = "Nullable::is_absent")]
    pub team_id: Nullable<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<String>,
}

/// Response from GET /teams
#[derive(Debug, Deserialize, Serialize)]
pub struct Team {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TriageRun {
    pub id: i64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(
        default,
        deserialize_with = "string_or_u64",
        rename = "total_processed"
    )]
    pub processed_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub flagged_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub dismissed_count: Option<u64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub cost_cents: Option<f64>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

// --- Eval ---

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalRun {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_cases: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub passed_cases: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub failed_cases: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub total_score: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub duration_ms: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalRunDetail {
    #[serde(flatten)]
    pub run: EvalRun,
    #[serde(default)]
    pub items: Option<Vec<EvalItem>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalItem {
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub case: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub score: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub duration_seconds: Option<f64>,
    #[serde(default)]
    pub trace_langfuse_id: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub conversation_log: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalRevision {
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub revision_message: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub latest_started_at: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub runs_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_score: Option<f64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_passed: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_failed: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalSuite {
    #[serde(default)]
    pub suite_key: Option<String>,
    #[serde(default)]
    pub suite_name: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub run_count: Option<u64>,
    #[serde(default)]
    pub last_run_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalCase {
    #[serde(default)]
    pub suite_key: Option<String>,
    #[serde(default)]
    pub suite_name: Option<String>,
    #[serde(default)]
    pub case_key: Option<String>,
    #[serde(default)]
    pub case_name: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub run_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub pass_rate: Option<f64>,
    #[serde(default)]
    pub last_run_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EvalFlaky {
    #[serde(default)]
    pub suite_key: Option<String>,
    #[serde(default)]
    pub suite_name: Option<String>,
    #[serde(default)]
    pub case_key: Option<String>,
    #[serde(default)]
    pub case_name: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub sample_size: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub passed_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub pass_rate: Option<f64>,
}

// --- Search ---

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchResult {
    #[serde(flatten)]
    pub trace: Trace,
    #[serde(default)]
    pub match_type: Option<String>,
    #[serde(default)]
    pub match_context: Option<String>,
}

// --- Features ---

/// A reference to a named entity (category, team, etc.)
#[derive(Debug, Deserialize, Serialize)]
pub struct NamedRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Feature {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<NamedRef>,
    #[serde(default)]
    pub teams: Option<Vec<NamedRef>>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub queue_item_count: Option<u64>,
}

// --- Flags ---

#[derive(Debug, Deserialize, Serialize)]
pub struct FlagInfo {
    pub flag_name: String,
    pub trace_count: u64,
    #[serde(default)]
    pub first_seen: Option<String>,
    #[serde(default)]
    pub last_seen: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CohortStats {
    pub trace_count: u64,
    #[serde(default)]
    pub cost: Option<CohortCost>,
    #[serde(default)]
    pub latency_ms: Option<CohortLatency>,
    #[serde(default)]
    pub errors: Option<u64>,
    #[serde(default)]
    pub tokens: Option<CohortTokens>,
    #[serde(default)]
    pub tool_calls: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CohortCost {
    #[serde(default)]
    pub total: f64,
    #[serde(default)]
    pub avg: f64,
    #[serde(default)]
    pub p_50: f64,
    #[serde(default)]
    pub p_95: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CohortLatency {
    #[serde(default)]
    pub avg: f64,
    #[serde(default)]
    pub p_50: f64,
    #[serde(default)]
    pub p_95: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CohortTokens {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FlagStatsResponse {
    pub flag_name: String,
    pub from: String,
    pub to: String,
    pub on: CohortStats,
    pub off: CohortStats,
}

// --- Trace Metrics (scoring) ---

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceMetricAggregate {
    pub group: String,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub trace_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_turn_count: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_tool_calls: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub error_rate: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_describe_resource_tokens: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub retry_pattern_rate: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_input_tokens: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_output_tokens: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_cost_usd: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub avg_latency_ms: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub success_rate: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceMetricAggregateResponse {
    pub data: Vec<TraceMetricAggregate>,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceMetricDetail {
    #[serde(default)]
    pub trace: Option<TraceMetricDetailTrace>,
    #[serde(default)]
    pub metrics: Option<TraceMetricDetailMetrics>,
    #[serde(default)]
    pub flags: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub triage: Option<TraceMetricDetailTriage>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceMetricDetailTrace {
    #[serde(default)]
    pub langfuse_id: Option<String>,
    #[serde(default)]
    pub user_query: Option<String>,
    #[serde(default)]
    pub agent_response: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub cost_usd: Option<f64>,
    #[serde(default, deserialize_with = "string_or_f64")]
    pub latency_ms: Option<f64>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceMetricDetailMetrics {
    #[serde(default, deserialize_with = "string_or_u64")]
    pub turn_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub tool_call_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub tool_error_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub unique_tool_count: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub total_output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub tool_input_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub tool_output_tokens: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub describe_resource_calls: Option<u64>,
    #[serde(default, deserialize_with = "string_or_u64")]
    pub describe_resource_tokens: Option<u64>,
    #[serde(default)]
    pub tool_breakdown: Option<serde_json::Value>,
    #[serde(default)]
    pub has_retry_pattern: Option<bool>,
    #[serde(default)]
    pub has_errors: Option<bool>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TraceMetricDetailTriage {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub confidence: Option<String>,
}

// --- Tags ---

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserialize_trace_metric_aggregate_response() {
        let payload = json!({
            "data": [
                {
                    "group": "2026-04-15",
                    "trace_count": 2075,
                    "avg_turn_count": 3.14,
                    "avg_tool_calls": 3.3,
                    "error_rate": 0.005,
                    "avg_describe_resource_tokens": 1200.0,
                    "retry_pattern_rate": 0.201,
                    "avg_input_tokens": 5000.0,
                    "avg_output_tokens": 800.0,
                    "avg_cost_usd": 0.22,
                    "avg_latency_ms": 16874.0,
                    "success_rate": 0.944
                },
                {
                    "group": "2026-04-16",
                    "trace_count": 100,
                    "success_rate": null
                }
            ],
            "meta": { "from": "2026-04-15", "to": "2026-04-16", "total_traces": 2175 }
        });

        let resp: TraceMetricAggregateResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].group, "2026-04-15");
        assert_eq!(resp.data[0].trace_count, Some(2075));
        assert!((resp.data[0].avg_turn_count.unwrap() - 3.14).abs() < 0.01);
        assert!((resp.data[0].success_rate.unwrap() - 0.944).abs() < 0.001);
        assert_eq!(resp.data[1].success_rate, None);
        assert!(resp.meta.is_some());
    }

    #[test]
    fn deserialize_trace_metric_detail() {
        let payload = json!({
            "trace": {
                "langfuse_id": "abc123",
                "user_query": "Show me revenue",
                "cost_usd": 0.11,
                "latency_ms": 20479.0,
                "environment": "default",
                "timestamp": "2026-04-16T06:48:32.202Z"
            },
            "metrics": {
                "turn_count": 3,
                "tool_call_count": 2,
                "tool_error_count": 0,
                "unique_tool_count": 2,
                "total_input_tokens": 5000,
                "total_output_tokens": 800,
                "describe_resource_calls": 1,
                "describe_resource_tokens": 400,
                "tool_breakdown": {"query_resources": {"calls": 1, "errors": 0}},
                "has_retry_pattern": false,
                "has_errors": false,
                "agent_type": "DiscoveryAgent",
                "outcome": "successful"
            },
            "flags": { "aiAgentDiscoveryAgent": true, "aiApiLatest": false },
            "triage": { "category": "feature_request", "confidence": "high" }
        });

        let detail: TraceMetricDetail = serde_json::from_value(payload).unwrap();

        let trace = detail.trace.unwrap();
        assert_eq!(trace.langfuse_id.unwrap(), "abc123");
        assert!((trace.cost_usd.unwrap() - 0.11).abs() < 0.001);

        let metrics = detail.metrics.unwrap();
        assert_eq!(metrics.turn_count, Some(3));
        assert_eq!(metrics.tool_call_count, Some(2));
        assert_eq!(metrics.tool_error_count, Some(0));
        assert_eq!(metrics.outcome.as_deref(), Some("successful"));
        assert_eq!(metrics.has_retry_pattern, Some(false));
        assert!(metrics.tool_breakdown.is_some());

        let flags = detail.flags.unwrap();
        assert_eq!(flags.len(), 2);
        assert_eq!(flags["aiAgentDiscoveryAgent"], json!(true));

        let triage = detail.triage.unwrap();
        assert_eq!(triage.category.as_deref(), Some("feature_request"));
    }

    #[test]
    fn deserialize_trace_metric_detail_missing_metrics() {
        let payload = json!({
            "trace": { "langfuse_id": "xyz" },
            "metrics": null,
            "flags": {},
            "triage": null
        });

        let detail: TraceMetricDetail = serde_json::from_value(payload).unwrap();
        assert!(detail.trace.is_some());
        assert!(detail.metrics.is_none());
        assert!(detail.triage.is_none());
    }
}
