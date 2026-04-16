use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::commands::util::open_url;
use crate::core::model::{BoardState, Column, Pr};
use crate::error::{Error, Result};
use crate::tui::columns;

/// Five columns in left-to-right kanban order.
const ORDER: [Column; 5] = [
    Column::DraftMine,
    Column::ReviewMine,
    Column::ReadyToMergeMine,
    Column::WaitingOnMe,
    Column::WaitingOnAuthor,
];

pub struct App {
    state: BoardState,
    /// Index into `ORDER` — which column is focused.
    pub focused: usize,
    /// Selected card index inside each column (len == ORDER.len()).
    pub selected: [usize; 5],
    quit: bool,
}

impl App {
    pub fn new(state: BoardState) -> Self {
        Self {
            state,
            focused: 0,
            selected: [0; 5],
            quit: false,
        }
    }

    pub fn board(&self) -> &BoardState {
        &self.state
    }

    pub fn columns_order() -> &'static [Column] {
        &ORDER
    }

    pub fn focused_column(&self) -> Column {
        ORDER[self.focused]
    }

    pub fn selected_index(&self, col: Column) -> usize {
        let idx = ORDER
            .iter()
            .position(|c| *c == col)
            .expect("column in ORDER");
        self.selected[idx]
    }

    pub fn column_prs(&self, col: Column) -> &[Pr] {
        match col {
            Column::DraftMine => &self.state.columns.draft_mine,
            Column::ReviewMine => &self.state.columns.review_mine,
            Column::ReadyToMergeMine => &self.state.columns.ready_to_merge_mine,
            Column::WaitingOnMe => &self.state.columns.waiting_on_me,
            Column::WaitingOnAuthor => &self.state.columns.waiting_on_author,
        }
    }

    fn focus_left(&mut self) {
        self.focused = (self.focused + ORDER.len() - 1) % ORDER.len();
    }

    fn focus_right(&mut self) {
        self.focused = (self.focused + 1) % ORDER.len();
    }

    fn move_up(&mut self) {
        let sel = &mut self.selected[self.focused];
        if *sel > 0 {
            *sel -= 1;
        }
    }

    fn move_down(&mut self) {
        let col = ORDER[self.focused];
        let len = self.column_prs(col).len();
        if len == 0 {
            return;
        }
        let sel = &mut self.selected[self.focused];
        if *sel + 1 < len {
            *sel += 1;
        }
    }

    fn clamp_selections(&mut self) {
        for (i, col) in ORDER.iter().enumerate() {
            let len = self.column_prs(*col).len();
            if len == 0 {
                self.selected[i] = 0;
            } else if self.selected[i] >= len {
                self.selected[i] = len - 1;
            }
        }
    }

    fn selected_pr(&self) -> Option<&Pr> {
        let col = self.focused_column();
        let prs = self.column_prs(col);
        prs.get(self.selected[self.focused])
    }

    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> Result<()> {
        if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
            self.quit = true;
            return Ok(());
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Left | KeyCode::Char('h') => self.focus_left(),
            KeyCode::Right | KeyCode::Char('l') => self.focus_right(),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Enter => {
                if let Some(pr) = self.selected_pr() {
                    open_url(&pr.url)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn run(state: BoardState) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut app = App::new(state);
    app.clamp_selections();

    let result = event_loop(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal
            .draw(|frame| columns::render(frame, app))
            .map_err(Error::Io)?;

        if !event::poll(Duration::from_millis(200)).map_err(Error::Io)? {
            continue;
        }
        if let Event::Key(key) = event::read().map_err(Error::Io)? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            app.handle_key(key.code, key.modifiers)?;
        }
    }
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().map_err(Error::Io)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(Error::Io)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(Error::Io)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode().map_err(Error::Io)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(Error::Io)?;
    terminal.show_cursor().map_err(Error::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{BoardState, ColumnsData, Pr, PrState, RottingBucket, SizeBucket};
    use chrono::Utc;

    fn sample_pr(repo: &str, number: u64, title: &str) -> Pr {
        Pr {
            number,
            repo: repo.to_string(),
            title: title.to_string(),
            url: format!("https://github.com/productiveio/{repo}/pull/{number}"),
            author: "ilucin".to_string(),
            state: PrState::Ready,
            created_at: Utc::now() - chrono::Duration::days(2),
            age_days: 2.0,
            size: Some(SizeBucket::M),
            rotting: RottingBucket::Warming,
            productive_task_id: None,
            comments_count: 0,
            base_branch: Some("main".to_string()),
            has_new_commits_since_my_review: None,
        }
    }

    fn sample_state() -> BoardState {
        BoardState {
            user: "ilucin".to_string(),
            fetched_at: Utc::now(),
            columns: ColumnsData {
                draft_mine: vec![sample_pr("ai-agent", 1, "Draft PR")],
                review_mine: vec![sample_pr("api", 2, "In review")],
                ready_to_merge_mine: vec![],
                waiting_on_me: vec![sample_pr("frontend", 3, "Please review")],
                waiting_on_author: vec![],
            },
        }
    }

    #[test]
    fn navigation_wraps_and_clamps() {
        let mut app = App::new(sample_state());
        app.clamp_selections();
        assert_eq!(app.focused, 0);

        app.focus_left(); // wraps to last
        assert_eq!(app.focused, 4);
        app.focus_right(); // wraps forward
        assert_eq!(app.focused, 0);

        // Moving down in empty column is a no-op.
        app.focused = 2; // ready_to_merge_mine — empty
        app.move_down();
        assert_eq!(app.selected[2], 0);

        // Moving down in a column with one item stays at 0.
        app.focused = 0;
        app.move_down();
        assert_eq!(app.selected[0], 0);
        app.move_up();
        assert_eq!(app.selected[0], 0);
    }

    #[test]
    fn renders_without_panic() {
        use crate::tui::columns;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(160, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(sample_state());
        app.clamp_selections();
        terminal
            .draw(|frame| columns::render(frame, &mut app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rendered: String = buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("tb-pr"));
        assert!(rendered.contains("Draft"));
        assert!(rendered.contains("Waiting on me"));
        assert!(rendered.contains("Please review"));
    }
}
