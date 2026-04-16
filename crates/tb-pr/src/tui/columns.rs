use chrono::Utc;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::commands::util::humanize_age_hours;
use crate::core::model::Column;
use crate::tui::app::App;
use crate::tui::card::{self, CARD_HEIGHT};

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
    render_footer(frame, chunks[2], app);

    if app.help_open {
        render_help(frame);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.board();
    let age_hours = (Utc::now() - state.fetched_at).num_seconds() as f64 / 3600.0;
    let age = humanize_age_hours(age_hours);

    let mut spans = vec![
        Span::styled("tb-pr", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::raw(format!("{}@productiveio", state.user)),
        Span::raw("  "),
        Span::styled(
            format!("refreshed {age} ago"),
            Style::default().add_modifier(Modifier::DIM),
        ),
    ];

    if app.is_fetching {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} fetching…", spinner_frame(app.tick_count)),
            Style::default().fg(Color::Cyan),
        ));
    } else if let Some(err) = &app.last_error {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("⚠ {err}"),
            Style::default().fg(Color::Red),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = if app.help_open {
        "?=close"
    } else {
        "←/→ column   ↑/↓ nav   ⏎=open  t=task  r=refresh  c=copy  ?=help  q=quit"
    };
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

fn render_column(frame: &mut Frame, area: Rect, app: &mut App, idx: usize, col: Column) {
    let count = app.column_prs(col).len();
    let is_focused = app.focused == idx;
    let selected = app.selected[idx].min(count.saturating_sub(1));
    let title = format!(" {} ({}) ", column_title(col), count);

    let block_style = if is_focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(block_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if count == 0 {
        let empty = Paragraph::new(Span::styled(
            "(empty)",
            Style::default().add_modifier(Modifier::DIM),
        ));
        frame.render_widget(empty, inner);
        return;
    }

    // Compute scroll offset so the selected card is visible.
    let visible = (inner.height / CARD_HEIGHT).max(1) as usize;
    let mut scroll_start = app.scroll[idx];
    if selected < scroll_start {
        scroll_start = selected;
    } else if selected >= scroll_start + visible {
        scroll_start = selected + 1 - visible;
    }
    app.scroll[idx] = scroll_start;

    let mut y = inner.y;
    let end = inner.y + inner.height;
    let prs = app.column_prs(col);
    for (i, pr) in prs.iter().enumerate().skip(scroll_start) {
        if y + CARD_HEIGHT > end {
            break;
        }
        let card_area = Rect::new(inner.x, y, inner.width, CARD_HEIGHT);
        let selected_here = is_focused && i == selected;
        card::render(frame, card_area, pr, selected_here);
        y += CARD_HEIGHT;
    }

    // Show a subtle "+N more" hint if we clipped.
    let shown = visible.min(count - scroll_start);
    let remaining = count.saturating_sub(scroll_start + shown);
    if remaining > 0 && y < end {
        let hint = Paragraph::new(Span::styled(
            format!("+{remaining} more ↓"),
            Style::default().add_modifier(Modifier::DIM),
        ));
        frame.render_widget(hint, Rect::new(inner.x, y, inner.width, 1));
    }
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

fn spinner_frame(tick: u64) -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick as usize) % FRAMES.len()]
}

fn render_help(frame: &mut Frame) {
    let area = centered_rect(60, 16, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title(" Keybinds ");
    let lines: Vec<Line> = [
        ("←/→  h/l", "switch column"),
        ("↑/↓  j/k", "move selection"),
        ("⏎    d  ", "open PR in browser"),
        ("t       ", "open Productive task"),
        ("r       ", "refresh now"),
        ("c       ", "copy PR URL"),
        ("?       ", "toggle this help"),
        ("q  Esc  ", "quit"),
    ]
    .iter()
    .map(|(k, v)| {
        Line::from(vec![
            Span::styled(format!(" {k}  "), Style::default().fg(Color::Cyan)),
            Span::raw(*v),
        ])
    })
    .collect();
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(x, y, w, h)
}
