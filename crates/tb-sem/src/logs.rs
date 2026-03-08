use std::collections::HashMap;

use regex::Regex;

use crate::api::LogEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioOutcome {
    Passed,
    Failed,
    RetriedPassed,
}

impl std::fmt::Display for ScenarioOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioOutcome::Passed => write!(f, "Passed"),
            ScenarioOutcome::Failed => write!(f, "Failed"),
            ScenarioOutcome::RetriedPassed => write!(f, "RetriedPassed"),
        }
    }
}

#[derive(Debug)]
pub struct ScenarioResult {
    pub name: String,
    pub feature_file: Option<String>,
    pub result: ScenarioOutcome,
    pub error_class: Option<ErrorClass>,
    pub error_detail: Option<String>,
    pub attempts: u32,
}

#[derive(Debug)]
pub struct FailureSummary {
    pub total_scenarios: u32,
    pub passed: u32,
    pub failed: u32,
    pub failures: Vec<FailedScenario>,
}

#[derive(Debug)]
pub struct FailedScenario {
    pub name: String,
    pub error_class: ErrorClass,
    pub error_detail: String,
}

#[derive(Debug, Clone)]
pub enum ErrorClass {
    Infra,
    Timeout,
    Assertion,
    Auth,
    Unknown,
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorClass::Infra => write!(f, "INFRA"),
            ErrorClass::Timeout => write!(f, "TIMEOUT"),
            ErrorClass::Assertion => write!(f, "ASSERTION"),
            ErrorClass::Auth => write!(f, "AUTH"),
            ErrorClass::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Flatten log events into plain text lines (only cmd_output).
pub fn flatten_log(events: &[LogEvent]) -> String {
    events
        .iter()
        .filter(|e| e.event == "cmd_output")
        .filter_map(|e| e.output.as_deref())
        .collect::<String>()
}

/// Extract cucumber summary from flattened log text.
pub fn parse_cucumber_summary(text: &str) -> Option<(u32, u32)> {
    let re = Regex::new(r"(\d+) scenarios \((\d+) failed, (\d+) passed\)").unwrap();
    re.captures(text).map(|caps| {
        let failed: u32 = caps[2].parse().unwrap_or(0);
        let passed: u32 = caps[3].parse().unwrap_or(0);
        (failed, passed)
    })
}

/// Classify error based on output text.
fn classify_error(text: &str) -> (ErrorClass, String) {
    if text.contains("502") || text.contains("Bad Gateway") {
        (ErrorClass::Infra, "502 Bad Gateway".to_string())
    } else if text.contains("503") || text.contains("Service Unavailable") {
        (ErrorClass::Infra, "503 Service Unavailable".to_string())
    } else if text.contains("ECONNRESET") || text.contains("ERR_CONNECTION_REFUSED") {
        (ErrorClass::Infra, "Connection error".to_string())
    } else if text.contains("API error") {
        (ErrorClass::Infra, "API error".to_string())
    } else if text.contains("TimeoutError") || text.contains("waiting for selector") {
        (ErrorClass::Timeout, "Timeout".to_string())
    } else if text.contains("element not found") || text.contains("timed out") {
        (ErrorClass::Timeout, "Element timeout".to_string())
    } else if text.contains("403 Forbidden") {
        (ErrorClass::Auth, "403 Forbidden".to_string())
    } else if text.contains("401 Unauthorized") {
        (ErrorClass::Auth, "401 Unauthorized".to_string())
    } else if text.contains("AssertionError") || text.contains("expected") {
        (ErrorClass::Assertion, "Assertion failure".to_string())
    } else {
        (ErrorClass::Unknown, String::new())
    }
}

/// Parse all scenarios from log events (passed, failed, retried).
/// Deduplicates by scenario name — keeps last attempt's result and tracks attempt count.
pub fn parse_all_scenarios(events: &[LogEvent]) -> Vec<ScenarioResult> {
    let text = flatten_log(events);
    let lines: Vec<&str> = text.lines().collect();

    // Track all scenario appearances: name -> list of (passed/failed, error_text)
    let mut scenario_attempts: HashMap<String, Vec<(bool, String, Option<String>)>> =
        HashMap::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.contains("Scenario:") && !is_summary_line(line) {
            let (name, feature_file) = parse_scenario_line(line);

            if !name.is_empty() {
                // Look ahead for pass/fail
                let mut j = i + 1;
                let mut found_failure = false;
                let mut error_text = String::new();

                while j < lines.len() && j < i + 30 {
                    let next_line = lines[j].trim();
                    if next_line.contains("Scenario:") {
                        break;
                    }
                    if next_line.contains("✖ failed") {
                        found_failure = true;
                    } else if found_failure && !next_line.is_empty() {
                        error_text.push_str(next_line);
                        error_text.push('\n');
                    }
                    j += 1;
                }

                scenario_attempts.entry(name).or_default().push((
                    found_failure,
                    error_text,
                    feature_file,
                ));

                i = j;
                continue;
            }
        }
        i += 1;
    }

    // Build results: for each scenario, determine outcome from all attempts.
    // In parallel/interleaved output, "last" isn't reliable — use pass/fail ratio.
    // If ANY attempt passed, the scenario passed (possibly retried).
    // Only mark as Failed if ALL attempts failed.
    let mut results = Vec::new();
    for (name, attempts) in &scenario_attempts {
        let attempt_count = attempts.len() as u32;
        let any_passed = attempts.iter().any(|(failed, _, _)| !*failed);
        let any_failed = attempts.iter().any(|(failed, _, _)| *failed);
        // Use the last failed attempt for error details
        let last_failed_attempt = attempts.iter().rev().find(|(failed, _, _)| *failed);
        let feature_file = attempts.last().unwrap().2.clone();

        let (outcome, error_class, error_detail) = if any_passed && any_failed {
            // Retried and eventually passed
            (ScenarioOutcome::RetriedPassed, None, None)
        } else if any_failed {
            // All attempts failed
            let (class, detail) = last_failed_attempt
                .map(|(_, err, _)| classify_error(err))
                .unwrap_or((ErrorClass::Unknown, String::new()));
            (ScenarioOutcome::Failed, Some(class), Some(detail))
        } else {
            (ScenarioOutcome::Passed, None, None)
        };

        results.push(ScenarioResult {
            name: name.clone(),
            feature_file,
            result: outcome,
            error_class,
            error_detail,
            attempts: attempt_count,
        });
    }

    // Sort: failed first, then retried, then passed
    results.sort_by_key(|r| match r.result {
        ScenarioOutcome::Failed => 0,
        ScenarioOutcome::RetriedPassed => 1,
        ScenarioOutcome::Passed => 2,
    });

    results
}

/// Parse failures from log events (backward-compatible with existing usage).
pub fn parse_failures(events: &[LogEvent]) -> FailureSummary {
    let text = flatten_log(events);
    let (failed_count, passed_count) = parse_cucumber_summary(&text).unwrap_or((0, 0));

    let all = parse_all_scenarios(events);
    let failures: Vec<FailedScenario> = all
        .iter()
        .filter(|s| s.result == ScenarioOutcome::Failed)
        .map(|s| FailedScenario {
            name: s.name.clone(),
            error_class: s.error_class.clone().unwrap_or(ErrorClass::Unknown),
            error_detail: s.error_detail.clone().unwrap_or_default(),
        })
        .collect();

    FailureSummary {
        total_scenarios: failed_count + passed_count,
        passed: passed_count,
        failed: failed_count,
        failures,
    }
}

/// Check if a line is from the cucumber summary section (e.g. "5) Scenario: ... (attempt 1, retried)")
fn is_summary_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Summary lines start with "N) Scenario:" where N is a number
    trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains(") Scenario:")
}

/// Parse scenario name and optional feature file from a "Scenario:" line.
fn parse_scenario_line(line: &str) -> (String, Option<String>) {
    let after = line.split("Scenario:").nth(1).unwrap_or("");
    let parts: Vec<&str> = after.split('#').collect();
    let name = parts[0].trim().to_string();
    let feature = if parts.len() > 1 {
        Some(parts[1].trim().to_string())
    } else {
        None
    };
    (name, feature)
}

/// A scenario from the cucumber numbered summary section at the end of logs.
#[derive(Debug, Clone)]
pub struct CucumberSummaryScenario {
    pub name: String,
    pub feature_file: Option<String>,
    pub attempt: u32,
    pub retried: bool,
}

/// Parse the cucumber numbered summary section.
/// Returns (true_failures, retried_passed) or None if no summary section found.
///
/// The summary section looks like:
///   1) Scenario: Name (attempt 6) # features/file.feature:31
///   2) Scenario: Name (attempt 1, retried) # features/file.feature:5
pub fn parse_cucumber_scenario_list(
    text: &str,
) -> Option<(Vec<CucumberSummaryScenario>, Vec<CucumberSummaryScenario>)> {
    let re = Regex::new(r"^\s*\d+\)\s+Scenario:\s+(.+)$").unwrap();
    let attempt_re = Regex::new(r"\(attempt (\d+)(?:, retried)?\)\s*$").unwrap();

    let mut failures = Vec::new();
    let mut retried = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !re.is_match(trimmed) {
            continue;
        }

        // Split at # for feature file
        let (scenario_part, feature_file) = if let Some(idx) = trimmed.rfind(" # ") {
            (&trimmed[..idx], Some(trimmed[idx + 3..].trim().to_string()))
        } else {
            (trimmed, None)
        };

        // Extract after "Scenario:"
        let after_scenario = scenario_part.split("Scenario:").nth(1).unwrap_or("").trim();

        // Extract attempt number and retried flag
        let (name, attempt, is_retried) = if let Some(caps) = attempt_re.captures(after_scenario) {
            let attempt: u32 = caps[1].parse().unwrap_or(1);
            let is_retried = after_scenario.contains(", retried)");
            let name = attempt_re.replace(after_scenario, "").trim().to_string();
            (name, attempt, is_retried)
        } else {
            (after_scenario.to_string(), 1, false)
        };

        if name.is_empty() {
            continue;
        }

        let entry = CucumberSummaryScenario {
            name,
            feature_file,
            attempt,
            retried: is_retried,
        };

        if is_retried {
            retried.push(entry);
        } else {
            failures.push(entry);
        }
    }

    if failures.is_empty() && retried.is_empty() {
        return None;
    }

    Some((failures, retried))
}

/// Build an error classification map by scanning inline scenario output.
/// This gives us error details even for scenarios that eventually passed on retry.
fn build_error_map_from_inline(
    events: &[LogEvent],
) -> HashMap<String, (Option<ErrorClass>, Option<String>)> {
    let text = flatten_log(events);
    let lines: Vec<&str> = text.lines().collect();
    let mut map: HashMap<String, (Option<ErrorClass>, Option<String>)> = HashMap::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.contains("Scenario:") && !is_summary_line(line) {
            let (name, _) = parse_scenario_line(line);

            if !name.is_empty() && !map.contains_key(&name) {
                // Look ahead for failure + error text
                let mut j = i + 1;
                let mut found_failure = false;
                let mut error_text = String::new();

                while j < lines.len() && j < i + 30 {
                    let next_line = lines[j].trim();
                    if next_line.contains("Scenario:") {
                        break;
                    }
                    if next_line.contains("✖ failed") {
                        found_failure = true;
                    } else if found_failure && !next_line.is_empty() {
                        error_text.push_str(next_line);
                        error_text.push('\n');
                    }
                    j += 1;
                }

                if found_failure {
                    let (class, detail) = classify_error(&error_text);
                    map.insert(name, (Some(class), Some(detail)));
                }

                i = j;
                continue;
            }
        }
        i += 1;
    }

    map
}

/// Best-effort scenario parsing: uses cucumber summary section as ground truth,
/// enriched with error classification from inline log parsing.
/// Falls back to `parse_all_scenarios` if no summary section is found.
pub fn parse_scenarios_best(events: &[LogEvent]) -> Vec<ScenarioResult> {
    let text = flatten_log(events);

    let Some((summary_failures, summary_retried)) = parse_cucumber_scenario_list(&text) else {
        return parse_all_scenarios(events);
    };

    // Build error map from inline log parsing (before dedup) for enrichment.
    // We classify errors from the raw inline scenario attempts so we can enrich
    // even scenarios that the inline parser marks as RetriedPassed.
    let error_map = build_error_map_from_inline(events);

    let mut results = Vec::new();

    // Deduplicate summary failures by name (keep highest attempt)
    let mut failure_map: HashMap<String, &CucumberSummaryScenario> = HashMap::new();
    for s in &summary_failures {
        let existing = failure_map.get(&s.name);
        if existing.is_none() || existing.unwrap().attempt < s.attempt {
            failure_map.insert(s.name.clone(), s);
        }
    }

    for s in failure_map.values() {
        let (error_class, error_detail) = error_map
            .get(&s.name)
            .cloned()
            .unwrap_or((Some(ErrorClass::Unknown), Some(String::new())));

        results.push(ScenarioResult {
            name: s.name.clone(),
            feature_file: s.feature_file.clone(),
            result: ScenarioOutcome::Failed,
            error_class,
            error_detail,
            attempts: s.attempt,
        });
    }

    // Deduplicate retried scenarios by name, count retry attempts
    let mut retried_counts: HashMap<String, (u32, &CucumberSummaryScenario)> = HashMap::new();
    for s in &summary_retried {
        let entry = retried_counts.entry(s.name.clone()).or_insert((0, s));
        entry.0 += 1;
    }
    // Only include retried scenarios that aren't also in the failure list
    for (name, (count, s)) in &retried_counts {
        if failure_map.contains_key(name) {
            continue;
        }
        results.push(ScenarioResult {
            name: s.name.clone(),
            feature_file: s.feature_file.clone(),
            result: ScenarioOutcome::RetriedPassed,
            error_class: None,
            error_detail: None,
            attempts: *count,
        });
    }

    // Sort: failed first, then retried, then passed
    results.sort_by_key(|r| match r.result {
        ScenarioOutcome::Failed => 0,
        ScenarioOutcome::RetriedPassed => 1,
        ScenarioOutcome::Passed => 2,
    });

    results
}
