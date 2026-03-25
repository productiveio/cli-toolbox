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

// --- Tags ---
