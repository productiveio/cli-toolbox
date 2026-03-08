use chrono::Utc;

/// Reusable time range arguments for commands that filter by date.
#[derive(clap::Args, Default)]
pub struct TimeRange {
    /// Relative time range (e.g., "7d", "2w", "30d")
    #[arg(long)]
    pub since: Option<String>,

    /// Start date (YYYY-MM-DD)
    #[arg(long)]
    pub from: Option<String>,

    /// End date (YYYY-MM-DD)
    #[arg(long)]
    pub to: Option<String>,
}

impl TimeRange {
    /// Convert to (from, to) query param strings.
    /// Bare dates in `--to` are treated as inclusive: `--to 2026-03-06` sends
    /// `to=2026-03-07` so the server's timestamp range covers the full day.
    pub fn resolve(&self) -> (Option<String>, Option<String>) {
        let from = if let Some(since) = &self.since {
            parse_relative(since)
        } else {
            self.from.clone()
        };
        let to = self.to.as_deref().map(make_to_inclusive);
        (from, to)
    }

    /// Add resolved time range params to a param list.
    pub fn push_params<'a>(&'a self, params: &mut Vec<(&'a str, Option<String>)>) {
        let (from, to) = self.resolve();
        params.push(("from", from));
        params.push(("to", to));
    }
}

/// If `to` is a bare date (YYYY-MM-DD), add one day so the server's
/// timestamp range covers the full day (making `--to` inclusive).
/// If it already contains a time component (`T`), return as-is.
fn make_to_inclusive(to: &str) -> String {
    if to.contains('T') {
        return to.to_string();
    }
    match chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d") {
        Ok(d) => (d + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string(),
        Err(_) => to.to_string(),
    }
}

fn parse_relative(s: &str) -> Option<String> {
    let s = s.trim();
    let (num_str, unit) = if let Some(n) = s.strip_suffix('d') {
        (n, 'd')
    } else if let Some(n) = s.strip_suffix('w') {
        (n, 'w')
    } else {
        return None;
    };

    let n: i64 = num_str.parse().ok()?;
    let days = match unit {
        'w' => n * 7,
        _ => n,
    };
    let date = Utc::now().date_naive() - chrono::Duration::days(days);
    Some(date.format("%Y-%m-%d").to_string())
}

/// Reusable pagination arguments.
#[derive(clap::Args)]
pub struct Pagination {
    /// Number of results per page
    #[arg(long, default_value = "20")]
    pub limit: u32,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: u32,
}

impl Pagination {
    pub fn push_params<'a>(&'a self, params: &mut Vec<(&'a str, Option<String>)>) {
        params.push(("per_page", Some(self.limit.to_string())));
        params.push(("page", Some(self.page.to_string())));
    }
}
