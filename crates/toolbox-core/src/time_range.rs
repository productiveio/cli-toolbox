use chrono::{NaiveDate, Utc};

/// Reusable time range arguments for commands that filter by date.
///
/// Primary flags: `--from` and `--to` (shown in help).
/// Hidden aliases: `--since`/`--after` → `--from`, `--before` → `--to`.
#[derive(clap::Args, Default, Clone, Debug)]
pub struct TimeRange {
    /// Start date — absolute (YYYY-MM-DD) or relative (7d, 2w, 24h, today, yesterday)
    #[arg(long)]
    pub from: Option<String>,

    /// End date, inclusive (YYYY-MM-DD or relative)
    #[arg(long)]
    pub to: Option<String>,

    #[arg(long, hide = true)]
    pub since: Option<String>,

    #[arg(long, hide = true)]
    pub after: Option<String>,

    #[arg(long, hide = true)]
    pub before: Option<String>,
}

/// Resolved time range with dates ready for API consumption.
///
/// When created via `resolve()`, `to` is the **exclusive** upper bound:
/// user input `--to 2026-03-10` resolves to `2026-03-11` so APIs with
/// `< to` / "before" semantics work correctly (Bugsnag, Semaphore).
///
/// When created via `resolve_inclusive()`, `to` is the user's literal date
/// with no offset — for APIs that handle end-of-day snapping server-side (DevPortal).
#[derive(Debug, Clone)]
pub struct ResolvedRange {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

impl TimeRange {
    /// Merge aliases and parse into dates.
    ///
    /// Errors if conflicting aliases are provided (e.g. both `--from` and `--since`).
    pub fn resolve(&self) -> Result<ResolvedRange, String> {
        let from_raw = merge_aliases(
            "--from",
            &[
                (&self.from, "--from"),
                (&self.since, "--since"),
                (&self.after, "--after"),
            ],
        )?;
        let to_raw = merge_aliases("--to", &[(&self.to, "--to"), (&self.before, "--before")])?;

        let from = from_raw.map(|s| parse_date(&s)).transpose()?;
        let to = to_raw
            .map(|s| parse_date(&s))
            .transpose()?
            .map(|d| d + chrono::Duration::days(1)); // make exclusive

        Ok(ResolvedRange { from, to })
    }

    /// Resolve, printing error and exiting on invalid input.
    /// Suitable for CLI entry points.
    pub fn resolve_or_exit(&self) -> ResolvedRange {
        unwrap_or_exit(self.resolve())
    }

    /// Resolve with **inclusive** `to` — no +1 day offset.
    /// Use for APIs that handle end-of-day snapping server-side (e.g. DevPortal).
    pub fn resolve_inclusive(&self) -> Result<ResolvedRange, String> {
        let mut r = self.resolve()?;
        r.to = r.to.map(|d| d - chrono::Duration::days(1));
        Ok(r)
    }

    /// Resolve inclusive, printing error and exiting on invalid input.
    pub fn resolve_inclusive_or_exit(&self) -> ResolvedRange {
        unwrap_or_exit(self.resolve_inclusive())
    }

    /// Returns true if any "from" alias (`--from`, `--since`, `--after`) is set.
    pub fn has_from(&self) -> bool {
        self.from.is_some() || self.since.is_some() || self.after.is_some()
    }

    /// Resolve inclusive and append `("from", ...)` and `("to", ...)` date string params.
    pub fn push_date_params_inclusive_or_exit(&self, params: &mut Vec<(&str, Option<String>)>) {
        self.resolve_inclusive_or_exit().push_date_params(params);
    }
}

impl ResolvedRange {
    /// Format as ISO8601 datetime strings (`2026-03-01T00:00:00Z`).
    pub fn to_iso8601(&self) -> (Option<String>, Option<String>) {
        (
            self.from.map(|d| format!("{}T00:00:00Z", d)),
            self.to.map(|d| format!("{}T00:00:00Z", d)),
        )
    }

    /// Format as Unix timestamps (seconds since epoch).
    pub fn to_timestamps(&self) -> (Option<i64>, Option<i64>) {
        (
            self.from
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc().timestamp()),
            self.to
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc().timestamp()),
        )
    }

    /// Format as date strings (`YYYY-MM-DD`).
    pub fn to_date_strings(&self) -> (Option<String>, Option<String>) {
        (
            self.from.map(|d| d.format("%Y-%m-%d").to_string()),
            self.to.map(|d| d.format("%Y-%m-%d").to_string()),
        )
    }

    /// Append `("from", ...)` and `("to", ...)` date string params to a list.
    fn push_date_params(&self, params: &mut Vec<(&str, Option<String>)>) {
        let (from, to) = self.to_date_strings();
        params.push(("from", from));
        params.push(("to", to));
    }
}

fn unwrap_or_exit(r: Result<ResolvedRange, String>) -> ResolvedRange {
    r.unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(2);
    })
}

/// Pick a single value from multiple alias sources, erroring on conflicts.
fn merge_aliases(
    canonical: &str,
    sources: &[(&Option<String>, &str)],
) -> Result<Option<String>, String> {
    let mut found: Option<(&str, &str)> = None;
    for (value, name) in sources {
        if let Some(v) = value {
            if let Some((prev_name, _)) = found {
                return Err(format!(
                    "conflicting flags: {} and {} both set (use {} only)",
                    prev_name, name, canonical
                ));
            }
            found = Some((name, v.as_str()));
        }
    }
    Ok(found.map(|(_, v)| v.to_string()))
}

/// Parse a date string — supports relative (`7d`, `2w`, `24h`, `today`, `yesterday`)
/// and absolute (`YYYY-MM-DD`) formats.
fn parse_date(s: &str) -> Result<NaiveDate, String> {
    let s = s.trim();

    match s {
        "today" => return Ok(Utc::now().date_naive()),
        "yesterday" => return Ok(Utc::now().date_naive() - chrono::Duration::days(1)),
        _ => {}
    }

    // Relative: Nd, Nw, Nh
    if let Some(num_str) = s.strip_suffix('d') {
        let n: i64 = num_str
            .parse()
            .map_err(|_| format!("invalid relative date: {}", s))?;
        return Ok(Utc::now().date_naive() - chrono::Duration::days(n));
    }
    if let Some(num_str) = s.strip_suffix('w') {
        let n: i64 = num_str
            .parse()
            .map_err(|_| format!("invalid relative date: {}", s))?;
        return Ok(Utc::now().date_naive() - chrono::Duration::days(n * 7));
    }
    if let Some(num_str) = s.strip_suffix('h') {
        let n: i64 = num_str
            .parse()
            .map_err(|_| format!("invalid relative date: {}", s))?;
        let dt = Utc::now() - chrono::Duration::hours(n);
        return Ok(dt.date_naive());
    }

    // Absolute: YYYY-MM-DD
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
        format!(
            "invalid date '{}' — expected YYYY-MM-DD or relative (7d, 2w, today, yesterday)",
            s
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_absolute_date() {
        assert_eq!(
            parse_date("2026-03-10").unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
        );
    }

    #[test]
    fn parse_relative_days() {
        let result = parse_date("7d").unwrap();
        let expected = Utc::now().date_naive() - chrono::Duration::days(7);
        assert_eq!(result, expected);
    }

    #[test]
    fn parse_relative_weeks() {
        let result = parse_date("2w").unwrap();
        let expected = Utc::now().date_naive() - chrono::Duration::days(14);
        assert_eq!(result, expected);
    }

    #[test]
    fn parse_relative_hours() {
        let result = parse_date("24h").unwrap();
        let expected = (Utc::now() - chrono::Duration::hours(24)).date_naive();
        assert_eq!(result, expected);
    }

    #[test]
    fn parse_today_yesterday() {
        assert_eq!(parse_date("today").unwrap(), Utc::now().date_naive());
        assert_eq!(
            parse_date("yesterday").unwrap(),
            Utc::now().date_naive() - chrono::Duration::days(1)
        );
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_date("not-a-date").is_err());
        assert!(parse_date("xd").is_err());
    }

    #[test]
    fn resolve_from_to() {
        let tr = TimeRange {
            from: Some("2026-03-01".into()),
            to: Some("2026-03-10".into()),
            ..Default::default()
        };
        let r = tr.resolve().unwrap();
        assert_eq!(
            r.from.unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
        );
        // to is exclusive: input 2026-03-10 → stored as 2026-03-11
        assert_eq!(r.to.unwrap(), NaiveDate::from_ymd_opt(2026, 3, 11).unwrap());
    }

    #[test]
    fn resolve_aliases() {
        let tr = TimeRange {
            since: Some("7d".into()),
            before: Some("2026-03-10".into()),
            ..Default::default()
        };
        let r = tr.resolve().unwrap();
        assert!(r.from.is_some());
        assert!(r.to.is_some());
    }

    #[test]
    fn resolve_conflicting_aliases_errors() {
        let tr = TimeRange {
            from: Some("7d".into()),
            since: Some("3d".into()),
            ..Default::default()
        };
        assert!(tr.resolve().is_err());
    }

    #[test]
    fn to_iso8601_format() {
        let r = ResolvedRange {
            from: Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()),
            to: Some(NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        };
        let (from, to) = r.to_iso8601();
        assert_eq!(from.unwrap(), "2026-03-01T00:00:00Z");
        assert_eq!(to.unwrap(), "2026-03-11T00:00:00Z");
    }

    #[test]
    fn to_timestamps_format() {
        let r = ResolvedRange {
            from: Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()),
            to: None,
        };
        let (from, to) = r.to_timestamps();
        assert!(from.unwrap() > 0);
        assert!(to.is_none());
    }

    #[test]
    fn to_date_strings_format() {
        let r = ResolvedRange {
            from: Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()),
            to: Some(NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        };
        let (from, to) = r.to_date_strings();
        assert_eq!(from.unwrap(), "2026-03-01");
        assert_eq!(to.unwrap(), "2026-03-11");
    }

    #[test]
    fn resolve_inclusive_no_offset() {
        let tr = TimeRange {
            from: Some("2026-03-01".into()),
            to: Some("2026-03-10".into()),
            ..Default::default()
        };
        let r = tr.resolve_inclusive().unwrap();
        assert_eq!(
            r.from.unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
        );
        // to is inclusive: input 2026-03-10 → stored as 2026-03-10 (no +1 day)
        assert_eq!(r.to.unwrap(), NaiveDate::from_ymd_opt(2026, 3, 10).unwrap());
    }

    #[test]
    fn has_from_detects_aliases() {
        let empty = TimeRange::default();
        assert!(!empty.has_from());

        let with_from = TimeRange {
            from: Some("7d".into()),
            ..Default::default()
        };
        assert!(with_from.has_from());

        let with_since = TimeRange {
            since: Some("7d".into()),
            ..Default::default()
        };
        assert!(with_since.has_from());

        let with_after = TimeRange {
            after: Some("7d".into()),
            ..Default::default()
        };
        assert!(with_after.has_from());
    }
}
