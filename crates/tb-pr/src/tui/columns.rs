use chrono::Utc;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::commands::util::humanize_age_hours;
use crate::core::model::Column;
use crate::tui::app::App;
use crate::tui::card;

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_columns(frame, chunks[1], app);
    render_footer(frame, chunks[2]);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.board();
    let age = humanize_age_hours((Utc::now() - state.fetched_at).num_seconds() as f64 / 3600.0);
    let line = Line::from(vec![
        Span::styled("tb-pr", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::raw(format!("{}@{}", state.user, "productiveio")),
        Span::raw("  "),
        Span::styled(
            format!("refreshed {age} ago"),
            Style::default().add_modifier(Modifier::DIM),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let help = "←/→ h/l column   ↑/↓ j/k nav   enter=open   q=quit";
    frame.render_widget(
        Paragraph::new(Span::styled(
            help,
            Style::default().add_modifier(Modifier::DIM),
        )),
        area,
    );
}

fn render_columns(frame: &mut Frame, area: Rect, app: &mut App) {
    let order = App::columns_order();
    let constraints: Vec<Constraint> = order
        .iter()
        .map(|_| Constraint::Ratio(1, order.len() as u32))
        .collect();
    let slots = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, col) in order.iter().enumerate() {
        render_column(frame, slots[i], app, i, *col);
    }
}

fn render_column(frame: &mut Frame, area: Rect, app: &App, idx: usize, col: Column) {
    let prs = app.column_prs(col);
    let is_focused = app.focused == idx;
    let title = format!(" {} ({}) ", column_title(col), prs.len());
    let mut block = Block::default().borders(Borders::ALL).title(title);
    if is_focused {
        block = block.border_style(Style::default().add_modifier(Modifier::BOLD));
    }

    let items: Vec<ListItem> = prs.iter().map(card::render).collect();
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED),
    );

    let mut list_state = ListState::default();
    if !prs.is_empty() {
        list_state.select(Some(app.selected_index(col).min(prs.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn column_title(col: Column) -> &'static str {
    match col {
        Column::DraftMine => "Draft",
        Column::ReviewMine => "In review",
        Column::ReadyToMergeMine => "Ready to merge",
        Column::WaitingOnMe => "Waiting on me",
        Column::WaitingOnAuthor => "Waiting on author",
    }
}
