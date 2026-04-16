use chrono::Utc;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

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
    } else if let Some(msg) = &app.status {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("✓ {msg}"),
            Style::default().fg(Color::Green),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = if app.help_open {
        "?=close"
    } else {
        "←/→ column   ↑/↓ nav   ⏎=open  t=task  r=refresh  c=copy  m=mark-all-read  w=wrap  ?=help  q=quit"
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
    if col == Column::Mentions {
        render_mentions_column(frame, area, app, idx);
        return;
    }

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

    // Adjust scroll offset so the selected card is fully visible.
    let full_titles = app.full_titles;
    let heights: Vec<u16> = app
        .column_prs(col)
        .iter()
        .map(|pr| card::card_height(pr, full_titles, inner.width))
        .collect();
    let mut scroll_start = app.scroll[idx].min(selected);
    while !range_fits(&heights, scroll_start, selected, inner.height) {
        scroll_start += 1;
    }
    app.scroll[idx] = scroll_start;

    let mut y = inner.y;
    let end = inner.y + inner.height;
    // Reserve the last row for the "+N more" overflow hint so we never
    // push it past the column bounds.
    let card_end = end.saturating_sub(1);
    let prs = app.column_prs(col);
    let mut last_drawn = scroll_start;
    for (i, pr) in prs.iter().enumerate().skip(scroll_start) {
        let h = heights[i];
        if y + h > card_end && i != scroll_start {
            break;
        }
        let card_area = Rect::new(inner.x, y, inner.width, h);
        let selected_here = is_focused && i == selected;
        card::render(frame, card_area, pr, selected_here, full_titles);
        y += h;
        last_drawn = i;
    }

    let remaining = count.saturating_sub(last_drawn + 1);
    if remaining > 0 && y < end {
        let hint = Paragraph::new(Span::styled(
            format!("+{remaining} more ↓"),
            Style::default().add_modifier(Modifier::DIM),
        ));
        frame.render_widget(hint, Rect::new(inner.x, y, inner.width, 1));
    }
}

/// Do cards `[start..=target]` (inclusive) all fit within `max_height`?
/// The selected card must land fully inside the column, otherwise we scroll.
fn range_fits(heights: &[u16], start: usize, target: usize, max_height: u16) -> bool {
    if start > target {
        return true;
    }
    let mut total: u16 = 0;
    for h in &heights[start..=target] {
        total = total.saturating_add(*h);
        if total > max_height {
            return false;
        }
    }
    true
}

fn column_title(col: Column) -> &'static str {
    match col {
        Column::DraftMine => "Draft",
        Column::ReviewMine => "In review",
        Column::ReadyToMergeMine => "Ready to merge",
        Column::WaitingOnMe => "Waiting on me",
        Column::WaitingOnAuthor => "Waiting on author",
        Column::Mentions => "Mentions",
    }
}

fn render_mentions_column(frame: &mut Frame, area: Rect, app: &mut App, idx: usize) {
    let count = app.column_notifications().len();
    let is_focused = app.focused == idx;
    let selected = app.selected[idx].min(count.saturating_sub(1));
    let title = format!(" {} ({}) ", column_title(Column::Mentions), count);

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
            "(inbox zero)",
            Style::default().add_modifier(Modifier::DIM),
        ));
        frame.render_widget(empty, inner);
        return;
    }

    let full_titles = app.full_titles;
    let heights: Vec<u16> = app
        .column_notifications()
        .iter()
        .map(|n| card::notification_card_height(n, full_titles, inner.width))
        .collect();
    let mut scroll_start = app.scroll[idx].min(selected);
    while !range_fits(&heights, scroll_start, selected, inner.height) {
        scroll_start += 1;
    }
    app.scroll[idx] = scroll_start;

    let mut y = inner.y;
    let end = inner.y + inner.height;
    let card_end = end.saturating_sub(1);
    let notifications = app.column_notifications();
    let mut last_drawn = scroll_start;
    for (i, notification) in notifications.iter().enumerate().skip(scroll_start) {
        let h = heights[i];
        if y + h > card_end && i != scroll_start {
            break;
        }
        let card_area = Rect::new(inner.x, y, inner.width, h);
        let selected_here = is_focused && i == selected;
        card::render_notification(frame, card_area, notification, selected_here, full_titles);
        y += h;
        last_drawn = i;
    }

    let remaining = count.saturating_sub(last_drawn + 1);
    if remaining > 0 && y < end {
        let hint = Paragraph::new(Span::styled(
            format!("+{remaining} more ↓"),
            Style::default().add_modifier(Modifier::DIM),
        ));
        frame.render_widget(hint, Rect::new(inner.x, y, inner.width, 1));
    }
}

fn spinner_frame(tick: u64) -> &'static str {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(tick as usize) % FRAMES.len()]
}

fn render_help(frame: &mut Frame) {
    let area = centered_rect(64, 20, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title(" Keybinds ");
    let lines: Vec<Line> = [
        ("←/→  h/l", "switch column"),
        ("↑/↓  j/k", "move selection"),
        ("⏎    d  ", "open PR / comment (marks notif. read)"),
        ("t       ", "open Productive task"),
        ("r       ", "refresh now"),
        ("c       ", "copy PR URL"),
        ("m       ", "mark all notifications read"),
        ("w       ", "wrap — show full titles"),
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
