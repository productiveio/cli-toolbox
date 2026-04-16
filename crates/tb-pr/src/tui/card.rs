use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::commands::util::humanize_age_hours;
use crate::core::classifier;
use crate::core::model::{Column, Notification, NotificationReason, Pr, RottingBucket, SizeBucket};

/// Minimum card height (2 borders + 1 title + 2 meta rows).
pub const MIN_CARD_HEIGHT: u16 = 5;

/// Compute how many rows the card will occupy in the given interior width.
/// When `full_titles` is on, the title wraps across as many rows as it needs;
/// otherwise it stays on one truncated row.
pub fn card_height(pr: &Pr, full_titles: bool, column_width: u16) -> u16 {
    let inner_width = column_width.saturating_sub(2); // minus left/right border
    let new_prefix = if pr.has_new_commits_since_my_review == Some(true) {
        3
    } else {
        0
    };
    let title_width = inner_width.saturating_sub(new_prefix).max(1) as usize;
    let title_len = display_title(&pr.title).chars().count();
    let title_lines = if full_titles {
        title_len.div_ceil(title_width).max(1) as u16
    } else {
        1
    };
    // 2 borders + title_lines + 2 meta rows
    (2 + title_lines + 2).max(MIN_CARD_HEIGHT)
}

pub fn render(frame: &mut Frame, area: Rect, pr: &Pr, selected: bool, full_titles: bool) {
    let border_color = rotting_color(pr.rotting);
    let border_style = if selected {
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(border_color)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner vertically: title on top, 2 fixed-height meta rows on bottom.
    let title_rows = inner.height.saturating_sub(2);
    let title_area = Rect::new(inner.x, inner.y, inner.width, title_rows);
    let row2_area = Rect::new(inner.x, inner.y + title_rows, inner.width, 1);
    let row3_area = Rect::new(inner.x, inner.y + title_rows + 1, inner.width, 1);

    let display = display_title(&pr.title);
    let mut title_spans = Vec::new();
    if pr.has_new_commits_since_my_review == Some(true) {
        title_spans.push(Span::raw("🆕 "));
    }
    let title_body = if full_titles {
        display.to_string()
    } else {
        truncate(
            display,
            inner.width.saturating_sub(title_spans_width(&title_spans)) as usize,
        )
    };
    title_spans.push(Span::styled(
        title_body,
        Style::default().add_modifier(Modifier::BOLD),
    ));
    let mut title_paragraph = Paragraph::new(Line::from(title_spans));
    if full_titles {
        title_paragraph = title_paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(title_paragraph, title_area);

    let mut row2_spans = vec![Span::styled(
        format!("{}#{}", pr.repo, pr.number),
        Style::default().add_modifier(Modifier::DIM),
    )];
    if let Some(task) = &pr.productive_task_id {
        row2_spans.push(Span::raw(" "));
        row2_spans.push(Span::styled(
            format!("[P-{task}]"),
            Style::default().fg(Color::Cyan),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(row2_spans)), row2_area);

    let mut row3_spans = vec![
        Span::styled(size_text(pr.size), Style::default().fg(Color::Gray)),
        Span::raw("  "),
        Span::styled(
            humanize_age_hours(pr.age_days * 24.0),
            Style::default().fg(border_color),
        ),
    ];
    if pr.comments_count > 0 {
        row3_spans.push(Span::raw("  "));
        row3_spans.push(Span::raw(format!("💬 {}", pr.comments_count)));
    }
    frame.render_widget(Paragraph::new(Line::from(row3_spans)), row3_area);
}

/// Minimum rows a notification card occupies (2 border + 1 title + 1 meta).
pub const NOTIFICATION_MIN_HEIGHT: u16 = 4;

pub fn notification_card_height(
    notification: &Notification,
    full_titles: bool,
    column_width: u16,
) -> u16 {
    let inner_width = column_width.saturating_sub(2);
    let title_width = inner_width.max(1) as usize;
    let title_len = display_title(&notification.pr_title).chars().count();
    let title_lines = if full_titles {
        title_len.div_ceil(title_width).max(1) as u16
    } else {
        1
    };
    (2 + title_lines + 1).max(NOTIFICATION_MIN_HEIGHT)
}

pub fn render_notification(
    frame: &mut Frame,
    area: Rect,
    notification: &Notification,
    selected: bool,
    full_titles: bool,
) {
    let bucket = classifier::rotting_bucket(Column::Mentions, notification.age_days * 24.0);
    let border_color = rotting_color(bucket);
    let border_style = if selected {
        Style::default()
            .fg(border_color)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(border_color)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Title on top N rows, single meta row on the bottom.
    let title_rows = inner.height.saturating_sub(1);
    let title_area = Rect::new(inner.x, inner.y, inner.width, title_rows);
    let meta_area = Rect::new(inner.x, inner.y + title_rows, inner.width, 1);

    let display = display_title(&notification.pr_title);
    let title_body = if full_titles {
        display.to_string()
    } else {
        truncate(display, inner.width as usize)
    };
    let title_line = Line::from(Span::styled(
        title_body,
        Style::default().add_modifier(Modifier::BOLD),
    ));
    let mut title_paragraph = Paragraph::new(title_line);
    if full_titles {
        title_paragraph = title_paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(title_paragraph, title_area);

    let meta_spans = vec![
        Span::styled(
            reason_badge(&notification.reason).to_string(),
            Style::default().fg(reason_color(&notification.reason)),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{}#{}", notification.repo, notification.pr_number),
            Style::default().add_modifier(Modifier::DIM),
        ),
        Span::raw("  "),
        Span::styled(
            humanize_age_hours(notification.age_days * 24.0),
            Style::default().fg(border_color),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(meta_spans)), meta_area);
}

fn reason_badge(r: &NotificationReason) -> &str {
    match r {
        NotificationReason::Mention => "@me",
        NotificationReason::TeamMention => "@team",
        NotificationReason::Comment => "💬",
        NotificationReason::ReviewRequested => "👀",
        NotificationReason::Author => "author",
        NotificationReason::StateChange => "state",
        NotificationReason::Subscribed => "sub",
        NotificationReason::Other(_) => "?",
    }
}

fn reason_color(r: &NotificationReason) -> Color {
    match r {
        NotificationReason::Mention => Color::Cyan,
        NotificationReason::TeamMention => Color::Magenta,
        NotificationReason::Comment => Color::Yellow,
        NotificationReason::ReviewRequested => Color::Green,
        _ => Color::Gray,
    }
}

pub fn rotting_color(b: RottingBucket) -> Color {
    match b {
        RottingBucket::Fresh => Color::DarkGray,
        RottingBucket::Warming => Color::Green,
        RottingBucket::Stale => Color::Yellow,
        RottingBucket::Rotting => Color::Rgb(255, 165, 0),
        RottingBucket::Critical => Color::Red,
    }
}

/// Strip a conventional-commit prefix (`fix:`, `feat(scope):`, `refactor!:`, …)
/// for display in the TUI. Returns the original slice if the head doesn't
/// match a known prefix — we never invent or reorder text.
pub fn display_title(raw: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "fix", "feat", "feature", "refactor", "chore", "docs", "doc", "update", "test", "tests",
        "ci", "perf", "build", "style", "revert",
    ];

    let trimmed = raw.trim_start();
    let Some(colon_idx) = trimmed.find(':').filter(|&i| i > 0 && i < 32) else {
        return raw;
    };
    let head = &trimmed[..colon_idx];
    let head = head.strip_suffix('!').unwrap_or(head);
    let prefix_tag = match head.find('(') {
        Some(paren) if head.ends_with(')') => &head[..paren],
        Some(_) => return raw,
        None => head,
    };
    if !PREFIXES.iter().any(|p| p.eq_ignore_ascii_case(prefix_tag)) {
        return raw;
    }
    trimmed[colon_idx + 1..].trim_start()
}

fn size_text(s: Option<SizeBucket>) -> &'static str {
    match s {
        Some(SizeBucket::Xs) => "XS",
        Some(SizeBucket::S) => "S",
        Some(SizeBucket::M) => "M",
        Some(SizeBucket::L) => "L",
        Some(SizeBucket::Xl) => "XL",
        None => "-",
    }
}

fn title_spans_width(spans: &[Span<'_>]) -> u16 {
    spans.iter().map(|s| s.content.chars().count() as u16).sum()
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    let take = max.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_common_prefixes() {
        assert_eq!(display_title("fix: something broken"), "something broken");
        assert_eq!(display_title("feat: new thing"), "new thing");
        assert_eq!(display_title("feature: new thing"), "new thing");
        assert_eq!(display_title("refactor: rename foo"), "rename foo");
        assert_eq!(display_title("update: tweak"), "tweak");
    }

    #[test]
    fn strips_scoped_prefixes() {
        assert_eq!(display_title("fix(tb-pr): x"), "x");
        assert_eq!(display_title("feat(ai-agent): y"), "y");
    }

    #[test]
    fn strips_breaking_bang() {
        assert_eq!(display_title("feat!: breaking thing"), "breaking thing");
        assert_eq!(display_title("refactor(core)!: bang"), "bang");
    }

    #[test]
    fn leaves_unknown_prefixes_alone() {
        assert_eq!(display_title("WIP: draft"), "WIP: draft");
        assert_eq!(display_title("Add support for X"), "Add support for X");
        assert_eq!(display_title("fix something"), "fix something"); // no colon
    }

    #[test]
    fn leaves_colon_in_long_sentences_alone() {
        // "this is a sentence: with a colon" should not strip anything because
        // the head isn't a known prefix tag.
        assert_eq!(
            display_title("this is a sentence: with a colon"),
            "this is a sentence: with a colon"
        );
    }

    #[test]
    fn card_height_grows_with_wrap() {
        use crate::core::model::{PrState, RottingBucket, SizeBucket};
        use chrono::Utc;
        let pr = Pr {
            number: 1,
            repo: "x".into(),
            title: "a".repeat(120),
            url: "".into(),
            author: "".into(),
            state: PrState::Ready,
            created_at: Utc::now(),
            age_days: 1.0,
            size: Some(SizeBucket::S),
            rotting: RottingBucket::Fresh,
            productive_task_id: None,
            comments_count: 0,
            base_branch: None,
            has_new_commits_since_my_review: None,
        };
        // Column width 30, inner width 28, title 120 chars → 5 wrapped lines.
        // Height = 2 border + 5 title + 2 meta = 9.
        assert_eq!(card_height(&pr, true, 30), 9);
        // Off: always min height.
        assert_eq!(card_height(&pr, false, 30), MIN_CARD_HEIGHT);
    }
}
