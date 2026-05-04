use std::io::IsTerminal;

use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::{Result, TbBugError};

/// Mutation operations supported by the CLI.
#[derive(Copy, Clone)]
pub enum Op {
    Fix,
    Ignore,
    Discard,
}

impl Op {
    fn as_str(self) -> &'static str {
        match self {
            Op::Fix => "fix",
            Op::Ignore => "ignore",
            Op::Discard => "discard",
        }
    }

    fn past_tense(self) -> &'static str {
        match self {
            Op::Fix => "fixed",
            Op::Ignore => "ignored",
            Op::Discard => "discarded",
        }
    }
}

/// Snooze rule — either time-based or event-count-based.
pub enum SnoozeRule {
    Seconds(u64),
    Events(u64),
}

impl SnoozeRule {
    fn to_json(&self) -> serde_json::Value {
        match self {
            SnoozeRule::Seconds(s) => serde_json::json!({
                "type": "seconds",
                "seconds": s,
            }),
            SnoozeRule::Events(n) => serde_json::json!({
                "type": "event_count",
                "event_count": n,
            }),
        }
    }

    fn describe(&self) -> String {
        match self {
            SnoozeRule::Seconds(s) => {
                if s % 86_400 == 0 {
                    format!("{}d", s / 86_400)
                } else if s % 3_600 == 0 {
                    format!("{}h", s / 3_600)
                } else if s % 60 == 0 {
                    format!("{}m", s / 60)
                } else {
                    format!("{}s", s)
                }
            }
            SnoozeRule::Events(n) => {
                format!("{} more {}", n, if *n == 1 { "event" } else { "events" })
            }
        }
    }
}

/// Parse a duration like `7d`, `24h`, `30m`, `120s` into seconds.
pub fn parse_duration(input: &str) -> std::result::Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }
    let (num_part, unit) = match s.chars().last() {
        Some(c) if c.is_ascii_alphabetic() => (&s[..s.len() - 1], c),
        _ => return Err(format!("missing unit (use s/m/h/d): {input}")),
    };
    let n: u64 = num_part
        .parse()
        .map_err(|_| format!("invalid number in duration: {input}"))?;
    let mult: u64 = match unit {
        's' | 'S' => 1,
        'm' | 'M' => 60,
        'h' | 'H' => 3600,
        'd' | 'D' => 86_400,
        other => return Err(format!("unknown duration unit '{other}' in {input}")),
    };
    n.checked_mul(mult)
        .ok_or_else(|| format!("duration overflow: {input}"))
}

pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    error_ids: &[String],
    op: Op,
    yes: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    if matches!(op, Op::Discard) && !yes {
        confirm_destructive(error_ids.len(), project)?;
    }

    client
        .update_errors(project_id, error_ids, op.as_str(), None)
        .await?;

    println!(
        "Marked {} error{} as {} in '{}'.",
        error_ids.len(),
        if error_ids.len() == 1 { "" } else { "s" },
        op.past_tense(),
        project,
    );
    Ok(())
}

pub async fn run_snooze(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    error_ids: &[String],
    rule: SnoozeRule,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    let extra = serde_json::json!({ "snooze_rule": rule.to_json() });
    client
        .update_errors(project_id, error_ids, "snooze", Some(extra))
        .await?;

    println!(
        "Snoozed {} error{} for {} in '{}'.",
        error_ids.len(),
        if error_ids.len() == 1 { "" } else { "s" },
        rule.describe(),
        project,
    );
    Ok(())
}

fn confirm_destructive(count: usize, project: &str) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Err(TbBugError::Other(
            "discard is destructive — pass --yes to confirm in non-interactive mode".into(),
        ));
    }
    use std::io::Write;
    eprint!(
        "Discard {} error{} in '{}'? Future events will be dropped. [y/N]: ",
        count,
        if count == 1 { "" } else { "s" },
        project
    );
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|e| TbBugError::Other(format!("failed to read input: {e}")))?;
    let trimmed = answer.trim().to_ascii_lowercase();
    if trimmed == "y" || trimmed == "yes" {
        Ok(())
    } else {
        Err(TbBugError::Other("cancelled".into()))
    }
}
