use crate::config::Config;
use crate::core::github::{GhClient, fetch_board_state};
use crate::core::model::Column;
use crate::error::{Error, Result};

pub async fn run(column: Option<String>, stale_days: Option<u32>, json: bool) -> Result<()> {
    if !json {
        return Err(Error::Other(
            "pretty table lands in M4 — use `tb-pr list --json` for now".to_string(),
        ));
    }

    let filter_col = match column.as_deref() {
        Some(s) => Some(Column::parse(s).ok_or_else(|| {
            Error::Other(format!(
                "unknown column `{s}` — expected one of: draft-mine, review-mine, \
                 ready-to-merge-mine, waiting-on-me, waiting-on-author"
            ))
        })?),
        None => None,
    };

    let config = Config::load()?;
    let client = GhClient::new()?;
    let mut state = fetch_board_state(
        &client,
        &config.github.org,
        &config.productive.org_slug,
        Some(config.github.username_override.as_str()),
    )
    .await?;

    if let Some(min_days) = stale_days {
        let cutoff = min_days as f64;
        retain_stale(&mut state.columns.draft_mine, cutoff);
        retain_stale(&mut state.columns.review_mine, cutoff);
        retain_stale(&mut state.columns.ready_to_merge_mine, cutoff);
        retain_stale(&mut state.columns.waiting_on_me, cutoff);
        retain_stale(&mut state.columns.waiting_on_author, cutoff);
    }

    if let Some(col) = filter_col {
        blank_other_columns(&mut state.columns, col);
    }

    let out = serde_json::to_string_pretty(&state)?;
    println!("{out}");
    Ok(())
}

fn retain_stale(list: &mut Vec<crate::core::model::Pr>, min_days: f64) {
    list.retain(|p| p.age_days >= min_days);
}

fn blank_other_columns(columns: &mut crate::core::model::ColumnsData, keep: Column) {
    if keep != Column::DraftMine {
        columns.draft_mine.clear();
    }
    if keep != Column::ReviewMine {
        columns.review_mine.clear();
    }
    if keep != Column::ReadyToMergeMine {
        columns.ready_to_merge_mine.clear();
    }
    if keep != Column::WaitingOnMe {
        columns.waiting_on_me.clear();
    }
    if keep != Column::WaitingOnAuthor {
        columns.waiting_on_author.clear();
    }
}
