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
    pub fn resolve(&self) -> (Option<String>, Option<String>) {
        let from = if let Some(since) = &self.since {
            parse_relative(since)
        } else {
            self.from.clone()
        };
        (from, self.to.clone())
    }

    /// Add resolved time range params to a param list.
    pub fn push_params<'a>(&'a self, params: &mut Vec<(&'a str, Option<String>)>) {
        let (from, to) = self.resolve();
        params.push(("from", from));
        params.push(("to", to));
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
