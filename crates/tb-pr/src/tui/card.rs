use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::commands::util::humanize_age_hours;
use crate::core::model::{Pr, RottingBucket, SizeBucket};

/// Total screen rows used by a rendered card (2 border + 3 content).
pub const CARD_HEIGHT: u16 = 5;

pub fn render(frame: &mut Frame, area: Rect, pr: &Pr, selected: bool) {
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

    let mut title_spans = Vec::new();
    if pr.has_new_commits_since_my_review == Some(true) {
        title_spans.push(Span::raw("🆕 "));
    }
    title_spans.push(Span::styled(
        truncate(&pr.title, inner.width as usize),
        Style::default().add_modifier(Modifier::BOLD),
    ));

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

    let content = Paragraph::new(vec![
        Line::from(title_spans),
        Line::from(row2_spans),
        Line::from(row3_spans),
    ]);
    frame.render_widget(content, inner);
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
