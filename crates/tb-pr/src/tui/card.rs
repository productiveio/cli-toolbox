use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::commands::util::humanize_age_hours;
use crate::core::model::{Pr, SizeBucket};

pub fn render(pr: &Pr) -> ListItem<'static> {
    let title = pr.title.clone();
    let title_line = if pr.has_new_commits_since_my_review == Some(true) {
        Line::from(vec![
            Span::styled("🆕 ", Style::default()),
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
        ])
    } else {
        Line::from(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        ))
    };

    let meta = Line::from(vec![
        Span::styled(
            format!("{}#{}", pr.repo, pr.number),
            Style::default().add_modifier(Modifier::DIM),
        ),
        Span::raw("  "),
        Span::raw(size_text(pr.size)),
        Span::raw("  "),
        Span::raw(humanize_age_hours(pr.age_days * 24.0)),
    ]);

    // Two-row card, separated by an empty line for airflow.
    ListItem::new(vec![title_line, meta, Line::from("")])
}

fn size_text(s: Option<SizeBucket>) -> String {
    match s {
        Some(SizeBucket::Xs) => "XS".into(),
        Some(SizeBucket::S) => "S".into(),
        Some(SizeBucket::M) => "M".into(),
        Some(SizeBucket::L) => "L".into(),
        Some(SizeBucket::Xl) => "XL".into(),
        None => "-".into(),
    }
}
