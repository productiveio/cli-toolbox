//! Turning one [`Session`] into one [`ListItem`], at whatever width we were given.
//!
//! Rows are assembled **to fit**: every line ends up exactly `plan.width` columns
//! wide, padding included. That's not cosmetic — the selected row is highlighted
//! with a background sweep across its spans, and relying on the terminal to clip
//! an over-long `{:<8}`-padded span leaves the highlight ragged.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::dash::layout::{Density, Plan};
use crate::discovery::Session;
use crate::render::{
    ago, clean_text, column, fit, home_rel, status_words, terminal_label, width_of,
};

/// Background of the selected row. Dark enough to read through in both themes.
const SELECTED_BG: Color = Color::Indexed(236);

fn status_style(status: &str) -> Style {
    match status {
        "busy" => Style::default().fg(Color::Green),
        "idle" => Style::default().fg(Color::DarkGray),
        "waiting" => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::Gray),
    }
}

/// The `1`-`9`/`0` digit a row can be jumped to with, or a blank past the tenth.
fn gutter_char(idx: usize) -> char {
    match idx {
        0..=8 => (b'1' + idx as u8) as char,
        9 => '0',
        _ => ' ',
    }
}

/// Accumulates spans while tracking how many display columns are left.
struct RowBuf {
    spans: Vec<Span<'static>>,
    used: usize,
    width: usize,
}

impl RowBuf {
    fn new(width: usize) -> Self {
        Self {
            spans: Vec::new(),
            used: 0,
            width,
        }
    }

    fn room(&self) -> usize {
        self.width.saturating_sub(self.used)
    }

    /// Append, clipped to whatever room is left. A zero-width push is dropped.
    fn push(&mut self, text: impl AsRef<str>, style: Style) {
        let room = self.room();
        if room == 0 {
            return;
        }
        let text = fit(text.as_ref(), room);
        let w = width_of(&text);
        if w == 0 {
            return;
        }
        self.used += w;
        self.spans.push(Span::styled(text, style));
    }

    /// Pad with spaces up to `target` columns (no-op if already past it).
    fn fill_to(&mut self, target: usize) {
        let want = target.saturating_sub(self.used).min(self.room());
        if want > 0 {
            self.used += want;
            self.spans.push(Span::raw(" ".repeat(want)));
        }
    }

    /// Fill the rest of the line with spaces so the highlight covers it.
    fn fill(&mut self) {
        let room = self.room();
        if room > 0 {
            self.used += room;
            self.spans.push(Span::raw(" ".repeat(room)));
        }
    }

    fn finish(mut self, selected: bool) -> Line<'static> {
        self.fill();
        if selected {
            for s in &mut self.spans {
                s.style = s.style.bg(SELECTED_BG);
            }
        }
        Line::from(self.spans)
    }
}

/// Caret + optional jump digit + trailing space: `"▸1 "`, `" 3 "` or `"▸ "`.
fn prefix(p: &Plan, idx: usize, selected: bool) -> String {
    let caret = if selected { '▸' } else { ' ' };
    if p.gutter {
        format!("{caret}{} ", gutter_char(idx))
    } else {
        format!("{caret} ")
    }
}

/// The terminal name as it is actually *drawn*, glyph stripped — or `""` when
/// the row shows no terminal at all.
fn drawn_terminal(label: &str) -> &str {
    label
        .strip_prefix("⧉ ")
        .or_else(|| label.strip_prefix("▣ "))
        .unwrap_or("")
        .trim()
}

/// Lowercase, minus a trailing `-<digits>` disambiguator: `work-3` and `Work`
/// are the same name as far as a reader of the row is concerned.
fn normalize(name: &str) -> String {
    let name = name.trim().to_lowercase();
    match name.rsplit_once('-') {
        Some((head, tail))
            if !head.is_empty() && !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) =>
        {
            head.to_string()
        }
        _ => name,
    }
}

/// Would printing `base` next to `label` just say the same thing twice?
///
/// Compared against the label the row *draws* — `terminal_label` shows the tmux
/// session **or** the tab, tmux winning — because comparing against both fields
/// suppressed cwds that weren't duplicated at all. Near-misses count too: a
/// `work-3` pane in `~/Code/work` drew `⧉ work-3  work`, which is exactly the
/// redundancy this exists to remove.
fn echoes_terminal(base: &str, label: &str) -> bool {
    let term = normalize(drawn_terminal(label));
    let base = normalize(base);
    if term.is_empty() || base.is_empty() {
        return false;
    }
    term.starts_with(&base) || base.starts_with(&term)
}

/// Last path component of a (possibly `~`-relative) directory.
fn basename(cwd: &str) -> String {
    let rel = home_rel(cwd);
    std::path::Path::new(&rel)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(rel)
}

fn title_of(s: &Session) -> String {
    clean_text(s.title.as_deref().unwrap_or("(no prompt)"))
}

/// One session, rendered for the given plan. 1 or 2 lines; always exactly
/// `plan.width` columns wide.
pub fn session_item(s: &Session, p: &Plan, idx: usize, selected: bool) -> ListItem<'static> {
    ListItem::new(session_lines(s, p, idx, selected))
}

/// The lines behind [`session_item`]. Separate so tests can measure their real
/// display width — a rendered buffer pads the cell after a double-wide glyph,
/// which makes the drawn text look one column wider than it is.
pub fn session_lines(s: &Session, p: &Plan, idx: usize, selected: bool) -> Vec<Line<'static>> {
    if p.two_row {
        vec![
            two_row_head(s, p, idx, selected),
            two_row_context(s, p, selected),
        ]
    } else if p.density == Density::Tiny {
        vec![tiny(s, p, selected)]
    } else {
        vec![one_row(s, p, idx, selected)]
    }
}

/// `● name…` plus an age when there's anything left over.
fn tiny(s: &Session, p: &Plan, selected: bool) -> Line<'static> {
    let (dot, _) = status_words(&s.status);
    let mut r = RowBuf::new(p.width);
    let caret = if selected { '▸' } else { ' ' };
    r.push(format!("{caret}{dot} "), status_style(&s.status));
    // Under ~16 columns there is no honest way to show anything but the name.
    let age = if p.width >= 16 {
        format!(" {}", ago(s.updated_at))
    } else {
        String::new()
    };
    let name_room = r.room().saturating_sub(width_of(&age));
    r.push(
        fit(&s.label(), name_room),
        Style::default().add_modifier(Modifier::BOLD),
    );
    if !age.is_empty() {
        r.fill_to(p.width.saturating_sub(width_of(&age)));
        r.push(age, Style::default().fg(Color::DarkGray));
    }
    r.finish(selected)
}

/// Columnar single line: caret, dot, name, status, age, terminal, cwd, title.
fn one_row(s: &Session, p: &Plan, idx: usize, selected: bool) -> Line<'static> {
    let (dot, word) = status_words(&s.status);
    let st = status_style(&s.status);
    let mut r = RowBuf::new(p.width);

    r.push(
        prefix(p, idx, selected),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    r.push(format!("{dot} "), st);
    r.push(
        column(&s.label(), p.name_width.min(r.room())),
        Style::default().add_modifier(Modifier::BOLD),
    );
    r.push(" ", Style::default());
    if p.show_status_word {
        r.push(column(word, 9), st);
        r.push(" ", Style::default());
    }
    r.push(
        format!("{:>4}  ", ago(s.updated_at)),
        Style::default().fg(Color::DarkGray),
    );

    if r.room() > 8 {
        let cap = 18.min(r.room());
        r.push(
            column(&terminal_label(s), cap),
            Style::default().fg(Color::Cyan),
        );
        r.push(" ", Style::default());
    }
    if p.show_cwd && r.room() > 12 {
        let cwd = s.cwd.as_deref().map(home_rel).unwrap_or_else(|| "?".into());
        // Keep at least 8 columns for the title so it never vanishes entirely.
        let cap = r.room().saturating_sub(8).min(30);
        r.push(fit(&cwd, cap), Style::default().fg(Color::Blue));
        r.push("  ", Style::default());
    }
    if r.room() > 6 {
        r.push(title_of(s), Style::default().fg(Color::Gray));
    }
    r.finish(selected)
}

/// Line 1 of a two-row item: the name, big, with `status age` pinned right.
fn two_row_head(s: &Session, p: &Plan, idx: usize, selected: bool) -> Line<'static> {
    let (dot, word) = status_words(&s.status);
    let st = status_style(&s.status);
    let pre = prefix(p, idx, selected);
    let dotcell = format!("{dot} ");
    let left = width_of(&pre) + width_of(&dotcell);

    let age = ago(s.updated_at);
    // The right column wins over the name: `needs you` is the whole point of the
    // row, so a long name gets ellipsised rather than the status dropped. Only a
    // pane with under ~8 columns left for a name falls back to the age alone.
    let full = format!("{word}  {age}");
    let right = if width_of(&full) + left + 8 <= p.width {
        full
    } else if width_of(&age) + left + 4 <= p.width {
        age
    } else {
        String::new()
    };
    let name_cap = p
        .name_width
        .min(p.width.saturating_sub(left + 1 + width_of(&right)));
    let name = fit(&s.label(), name_cap);

    let mut r = RowBuf::new(p.width);
    r.push(
        pre,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    r.push(dotcell, st);
    r.push(name, Style::default().add_modifier(Modifier::BOLD));
    if !right.is_empty() {
        // Every two-row plan shows the status word, so the right column always
        // carries the status colour.
        r.fill_to(p.width.saturating_sub(width_of(&right)));
        r.push(right, st);
    }
    r.finish(selected)
}

/// Line 2 of a two-row item: which terminal, which directory, what it's doing.
fn two_row_context(s: &Session, p: &Plan, selected: bool) -> Line<'static> {
    let mut r = RowBuf::new(p.width);
    // Line up under the name on line 1: caret + digit + space + dot + space.
    r.push("     ", Style::default());
    let label = terminal_label(s);
    r.push(
        fit(&label, 20.min(r.room())),
        Style::default().fg(Color::Cyan),
    );
    r.push("  ", Style::default());
    if p.show_cwd && r.room() > 6 {
        let base = s.cwd.as_deref().map(basename).unwrap_or_else(|| "?".into());
        // One tmux session (or iTerm tab) per worktree is the convention, so the
        // basename is usually the terminal name repeated — spend the columns on
        // the title instead.
        if !echoes_terminal(&base, &label) {
            let cap = r.room().saturating_sub(4).min(18);
            r.push(fit(&base, cap), Style::default().fg(Color::Blue));
            r.push("  ", Style::default());
        }
    }
    if r.room() > 4 {
        r.push(title_of(s), Style::default().fg(Color::Gray));
    }
    r.finish(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::layout::plan;

    fn session(name: &str, status: &str) -> Session {
        Session {
            pid: 4242,
            session_id: Some("aafec1b4-e501-4177".into()),
            name: Some(name.into()),
            cwd: Some("/Users/x/Code/work/worktrees/ai-agent".into()),
            status: status.into(),
            updated_at: Some(chrono::Utc::now().timestamp_millis() - 120_000),
            tmux_session: Some("ai-agent".into()),
            title: Some("make the CDC backfill idempotent so a re-run is free".into()),
            ..Default::default()
        }
    }

    /// Render one item into a fresh buffer and read the drawn lines back.
    fn drawn(s: &Session, p: &Plan, idx: usize, selected: bool) -> Vec<String> {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::widgets::List;

        let h = p.item_height();
        let mut t = Terminal::new(TestBackend::new(p.width as u16, h)).unwrap();
        t.draw(|f| {
            f.render_widget(List::new(vec![session_item(s, p, idx, selected)]), f.area());
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..p.width as u16)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    const SIZES: [(u16, u16); 7] = [
        (200, 60),
        (120, 40),
        (80, 24),
        (50, 20),
        (40, 16),
        (30, 10),
        (20, 5),
    ];

    #[test]
    fn rows_never_exceed_the_inner_width() {
        let long = session("cdc-ingestion-backfill-and-then-some-more-words", "busy");
        for (w, h) in SIZES {
            let p = plan(w, h, 1, 46, None);
            for sel in [false, true] {
                for line in session_lines(&long, &p, 0, sel) {
                    assert_eq!(line.width(), p.width, "{w}x{h} selected={sel}: {line:?}");
                }
            }
        }
    }

    #[test]
    fn two_row_mode_draws_exactly_two_lines() {
        let p = plan(50, 20, 1, 22, None);
        assert!(p.two_row);
        let item = session_item(&session("cdc-backfill", "busy"), &p, 0, true);
        assert_eq!(item.height(), 2);
        let out = drawn(&session("cdc-backfill", "busy"), &p, 0, true);
        assert_eq!(out.len(), 2);
        // Line 1 = identity + state, line 2 = context.
        assert!(out[0].contains("cdc-backfill"), "{out:?}");
        assert!(out[0].contains("working"), "{out:?}");
        assert!(out[1].contains("ai-agent"), "{out:?}");
    }

    #[test]
    fn one_row_mode_draws_exactly_one_line() {
        let p = plan(120, 40, 1, 22, None);
        assert!(!p.two_row);
        let item = session_item(&session("cdc-backfill", "busy"), &p, 0, false);
        assert_eq!(item.height(), 1);
    }

    // The tmux session is the identifier Ivan actually names after the job; the
    // window name is almost always "zsh".
    #[test]
    fn the_tmux_session_is_on_the_row_at_every_size() {
        for (w, h) in SIZES.iter().take(6).copied() {
            let p = plan(w, h, 1, 12, None);
            let out = drawn(&session("cdc", "busy"), &p, 0, false).join("\n");
            assert!(out.contains("ai-agent"), "{w}x{h}: {out:?}");
            assert!(out.contains('⧉'), "{w}x{h}: {out:?}");
        }
    }

    #[test]
    fn a_name_that_fits_the_plan_is_not_truncated() {
        let name = "cdc-ingest-backfill"; // 19 columns
        for (w, h) in [(200u16, 60u16), (120, 40), (80, 24), (50, 20)] {
            let p = plan(w, h, 1, name.len(), None);
            let out = drawn(&session(name, "busy"), &p, 0, false).join("\n");
            assert!(out.contains(name), "{w}x{h}: {out:?}");
            assert!(!out.contains("cdc-ingest-backfil…"), "{w}x{h}: {out:?}");
        }
    }

    #[test]
    fn waiting_sessions_say_so_in_words() {
        let p = plan(50, 20, 1, 22, None);
        let out = drawn(&session("flag-cleanup", "waiting"), &p, 0, false).join("\n");
        assert!(out.contains("needs you"), "{out:?}");
        assert!(out.contains('⏸'), "{out:?}");
    }

    #[test]
    fn the_gutter_numbers_the_first_ten_rows() {
        let p = plan(50, 20, 12, 22, None);
        assert!(p.gutter);
        let s = session("x", "idle");
        assert!(drawn(&s, &p, 0, false)[0].starts_with(" 1 "));
        assert!(drawn(&s, &p, 0, true)[0].starts_with("▸1 "));
        assert!(drawn(&s, &p, 8, false)[0].starts_with(" 9 "));
        assert!(drawn(&s, &p, 9, false)[0].starts_with(" 0 "));
        // Past the tenth there is no digit to offer.
        assert!(drawn(&s, &p, 10, false)[0].starts_with("   "));
    }

    #[test]
    fn control_characters_in_a_title_are_stripped() {
        let mut s = session("x", "busy");
        s.title = Some("first\nsecond\u{1b}[31m\u{7}third".into());
        let p = plan(120, 40, 1, 12, None);
        let out = drawn(&s, &p, 0, false).join("\n");
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
        assert!(out.contains("first second"), "{out:?}");
    }

    #[test]
    fn sessions_with_no_terminal_still_render() {
        let mut s = session("orphan", "idle");
        s.tmux_session = None;
        s.tab = None;
        s.cwd = None;
        s.title = None;
        for (w, h) in [(120u16, 40u16), (50, 20), (20, 5)] {
            let p = plan(w, h, 1, 6, None);
            for line in session_lines(&s, &p, 0, true) {
                assert_eq!(line.width(), p.width);
            }
        }
    }

    // The dedup compares against the label the row draws — `terminal_label`
    // shows tmux *or* tab, tmux winning — not against both fields.
    #[test]
    fn the_cwd_is_dropped_only_when_it_repeats_the_drawn_terminal() {
        // Exact repeat: the tmux session is already on the row.
        assert!(echoes_terminal("ai-agent", "⧉ ai-agent"));
        // Under-suppression this used to miss: `⧉ work-3  work`.
        assert!(echoes_terminal("work", "⧉ work-3"));
        assert!(echoes_terminal("work-3", "⧉ work"));
        assert!(echoes_terminal("Work", "⧉ work-12"));
        // iTerm rows dedup against the tab, which is what they draw.
        assert!(echoes_terminal("sem", "▣ sem"));

        // Over-suppression this used to cause: the row draws the tmux session,
        // so a cwd matching only the (undrawn) window name is not a duplicate.
        assert!(!echoes_terminal("zsh", "⧉ ai-agent"));
        assert!(!echoes_terminal("frontend", "⧉ ai-agent"));
        // Nothing to compare against.
        assert!(!echoes_terminal("work", "-"));
        assert!(!echoes_terminal("", "⧉ work"));
    }

    #[test]
    fn a_cwd_that_repeats_the_tmux_session_is_not_drawn_twice() {
        let p = plan(50, 20, 1, 22, None);
        assert!(p.two_row && p.show_cwd);

        // tmux `work-3` in ~/Code/work — the basename adds nothing.
        let mut s = session("statusline", "idle");
        s.tmux_session = Some("work-3".into());
        s.cwd = Some("/Users/x/Code/work".into());
        let out = drawn(&s, &p, 0, false).join("\n");
        assert!(out.contains("⧉ work-3"), "{out:?}");
        assert!(!out.contains("work-3  work"), "{out:?}");

        // …but a cwd that says something *is* drawn, even when the window name
        // (which the row never shows) happens to match it.
        let mut s = session("statusline", "idle");
        s.tmux_session = Some("ai-agent".into());
        s.tab = Some("toolbox".into());
        s.cwd = Some("/Users/x/Code/toolbox".into());
        let out = drawn(&s, &p, 0, false).join("\n");
        assert!(out.contains("⧉ ai-agent"), "{out:?}");
        assert!(out.contains("toolbox"), "{out:?}");
    }

    #[test]
    fn wide_names_do_not_shift_the_columns() {
        let s = session("测试-会话-🚀", "busy");
        assert_eq!(width_of("测试-会话-🚀"), 12);
        for (w, h) in SIZES {
            let p = plan(w, h, 1, 12, None);
            for line in session_lines(&s, &p, 0, false) {
                assert_eq!(line.width(), p.width, "{w}x{h}: {line:?}");
            }
        }
    }
}
