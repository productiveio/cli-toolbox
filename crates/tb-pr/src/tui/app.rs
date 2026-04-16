use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::commands::util::{copy_to_clipboard, open_url};
use crate::core::github::{GhClient, fetch_board_state};
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

const AUTO_REFRESH: Duration = Duration::from_secs(300);

/// What the app wants the event loop to do after handling a key.
#[derive(Debug, PartialEq, Eq)]
pub enum Intent {
    None,
    Quit,
    Refresh,
    OpenUrl(String),
    CopyUrl(String),
}

pub struct App {
    state: BoardState,
    /// Index into `ORDER` — which column is focused.
    pub focused: usize,
    /// Selected card index per column.
    pub selected: [usize; 5],
    /// First visible card index per column (for scrolling).
    pub scroll: [usize; 5],
    pub help_open: bool,
    pub full_titles: bool,
    pub is_fetching: bool,
    pub last_error: Option<String>,
    pub tick_count: u64,
    productive_org_slug: String,
}

impl App {
    pub fn new(state: BoardState, productive_org_slug: String) -> Self {
        Self {
            state,
            focused: 0,
            selected: [0; 5],
            scroll: [0; 5],
            help_open: false,
            full_titles: true,
            is_fetching: false,
            last_error: None,
            tick_count: 0,
            productive_org_slug,
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

    pub fn clamp_selections(&mut self) {
        for (i, col) in ORDER.iter().enumerate() {
            let len = self.column_prs(*col).len();
            if len == 0 {
                self.selected[i] = 0;
                self.scroll[i] = 0;
            } else if self.selected[i] >= len {
                self.selected[i] = len - 1;
            }
        }
    }

    fn selected_pr(&self) -> Option<&Pr> {
        let col = self.focused_column();
        self.column_prs(col).get(self.selected[self.focused])
    }

    fn task_url(&self, pr: &Pr) -> Option<String> {
        pr.productive_task_id.as_ref().map(|id| {
            format!(
                "https://app.productive.io/{}/tasks/{id}",
                self.productive_org_slug
            )
        })
    }

    pub fn replace_state(&mut self, state: BoardState) {
        self.state = state;
        self.last_error = None;
        self.is_fetching = false;
        self.clamp_selections();
    }

    pub fn mark_error(&mut self, err: String) {
        self.last_error = Some(err);
        self.is_fetching = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Intent {
        // Ctrl-C always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return Intent::Quit;
        }

        // Help popup swallows most input — only ? and quit keys work.
        if self.help_open {
            return match key.code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.help_open = false;
                    Intent::None
                }
                _ => Intent::None,
            };
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Intent::Quit,
            KeyCode::Char('?') => {
                self.help_open = true;
                Intent::None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.focus_left();
                Intent::None
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.focus_right();
                Intent::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                Intent::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                Intent::None
            }
            KeyCode::Enter | KeyCode::Char('d') => self
                .selected_pr()
                .map(|pr| Intent::OpenUrl(pr.url.clone()))
                .unwrap_or(Intent::None),
            KeyCode::Char('t') => {
                if let Some(pr) = self.selected_pr() {
                    if let Some(url) = self.task_url(pr) {
                        return Intent::OpenUrl(url);
                    }
                    self.last_error = Some("no Productive task linked on this PR".to_string());
                }
                Intent::None
            }
            KeyCode::Char('c') => self
                .selected_pr()
                .map(|pr| Intent::CopyUrl(pr.url.clone()))
                .unwrap_or(Intent::None),
            KeyCode::Char('r') => {
                if !self.is_fetching {
                    Intent::Refresh
                } else {
                    Intent::None
                }
            }
            KeyCode::Char('w') => {
                self.full_titles = !self.full_titles;
                // Scroll offsets may now be wrong for the new card heights;
                // reset so the selected card re-anchors on next render.
                self.scroll = [0; 5];
                Intent::None
            }
            _ => Intent::None,
        }
    }
}

/// Config passed in from the CLI — everything the background fetcher needs.
#[derive(Clone)]
pub struct FetchCtx {
    pub org: String,
    pub productive_org_slug: String,
    pub username_override: String,
}

#[derive(Debug)]
enum UiEvent {
    Key(KeyEvent),
    RefreshTick,
    FetchDone(std::result::Result<BoardState, String>),
    AnimTick,
}

pub async fn run(state: BoardState, ctx: FetchCtx) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let productive_slug = ctx.productive_org_slug.clone();
    let mut app = App::new(state, productive_slug);
    app.clamp_selections();

    let result = event_loop(&mut terminal, &mut app, ctx).await;
    restore_terminal(&mut terminal)?;
    result
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ctx: FetchCtx,
) -> Result<()> {
    let (tx, mut rx) = unbounded_channel::<UiEvent>();
    let ctx = Arc::new(ctx);

    // Keyboard pump — blocking crossterm on a dedicated thread.
    let tx_kb = tx.clone();
    std::thread::spawn(move || keyboard_pump(tx_kb));

    // Auto-refresh timer.
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(AUTO_REFRESH);
        interval.tick().await; // consume the immediate tick
        loop {
            interval.tick().await;
            if tx_tick.send(UiEvent::RefreshTick).is_err() {
                break;
            }
        }
    });

    // Spinner / "refreshed Nm ago" animation tick.
    let tx_anim = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(120));
        loop {
            interval.tick().await;
            if tx_anim.send(UiEvent::AnimTick).is_err() {
                break;
            }
        }
    });

    // Draw once up front.
    terminal
        .draw(|frame| columns::render(frame, app))
        .map_err(Error::Io)?;

    while let Some(event) = rx.recv().await {
        match event {
            UiEvent::Key(k) => match app.handle_key(k) {
                Intent::Quit => break,
                Intent::Refresh => spawn_fetch(&tx, &ctx, app),
                Intent::OpenUrl(url) => {
                    if let Err(e) = open_url(&url) {
                        app.mark_error(format!("open failed: {e}"));
                    }
                }
                Intent::CopyUrl(url) => match copy_to_clipboard(&url) {
                    Ok(()) => app.last_error = Some(format!("copied {url}")),
                    Err(e) => app.mark_error(format!("copy failed: {e}")),
                },
                Intent::None => {}
            },
            UiEvent::RefreshTick => {
                if !app.is_fetching {
                    spawn_fetch(&tx, &ctx, app);
                }
            }
            UiEvent::FetchDone(Ok(state)) => app.replace_state(state),
            UiEvent::FetchDone(Err(msg)) => app.mark_error(msg),
            UiEvent::AnimTick => {
                app.tick_count = app.tick_count.wrapping_add(1);
            }
        }
        terminal
            .draw(|frame| columns::render(frame, app))
            .map_err(Error::Io)?;
    }
    Ok(())
}

fn spawn_fetch(tx: &UnboundedSender<UiEvent>, ctx: &Arc<FetchCtx>, app: &mut App) {
    app.is_fetching = true;
    app.last_error = None;
    let tx = tx.clone();
    let ctx = ctx.clone();
    tokio::spawn(async move {
        let result = match GhClient::new() {
            Ok(client) => fetch_board_state(
                &client,
                &ctx.org,
                &ctx.productive_org_slug,
                Some(ctx.username_override.as_str()),
            )
            .await
            .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        let _ = tx.send(UiEvent::FetchDone(result));
    });
}

/// Blocking loop on a dedicated thread — crossterm's event reader is
/// blocking, so we bridge it into the tokio channel.
fn keyboard_pump(tx: UnboundedSender<UiEvent>) {
    loop {
        match event::poll(Duration::from_millis(200)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                    if tx.send(UiEvent::Key(k)).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
            Ok(false) => {
                if tx.is_closed() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
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
            productive_task_id: Some("1234".to_string()),
            comments_count: 3,
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

    fn app() -> App {
        let mut a = App::new(sample_state(), "109-productive".to_string());
        a.clamp_selections();
        a
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn navigation_wraps_and_clamps() {
        let mut a = app();
        assert_eq!(a.handle_key(key(KeyCode::Left)), Intent::None);
        assert_eq!(a.focused, 4);
        assert_eq!(a.handle_key(key(KeyCode::Right)), Intent::None);
        assert_eq!(a.focused, 0);

        a.focused = 2;
        a.handle_key(key(KeyCode::Down));
        assert_eq!(a.selected[2], 0); // empty column stays at 0
    }

    #[test]
    fn enter_and_d_return_open_url() {
        let mut a = app();
        let intent = a.handle_key(key(KeyCode::Enter));
        assert!(matches!(intent, Intent::OpenUrl(ref url) if url.contains("ai-agent")));
        let intent = a.handle_key(key(KeyCode::Char('d')));
        assert!(matches!(intent, Intent::OpenUrl(_)));
    }

    #[test]
    fn t_opens_productive_task_url_when_linked() {
        let mut a = app();
        let intent = a.handle_key(key(KeyCode::Char('t')));
        match intent {
            Intent::OpenUrl(url) => {
                assert!(url.contains("app.productive.io/109-productive/tasks/1234"));
            }
            other => panic!("expected OpenUrl, got {other:?}"),
        }
    }

    #[test]
    fn c_returns_copy_intent() {
        let mut a = app();
        match a.handle_key(key(KeyCode::Char('c'))) {
            Intent::CopyUrl(url) => assert!(url.contains("pull/1")),
            other => panic!("expected CopyUrl, got {other:?}"),
        }
    }

    #[test]
    fn help_popup_swallows_input_until_closed() {
        let mut a = app();
        a.handle_key(key(KeyCode::Char('?')));
        assert!(a.help_open);
        // Arrows do nothing while help is open.
        a.handle_key(key(KeyCode::Right));
        assert_eq!(a.focused, 0);
        // ? closes it again.
        a.handle_key(key(KeyCode::Char('?')));
        assert!(!a.help_open);
    }

    #[test]
    fn w_toggles_full_titles_and_resets_scroll() {
        let mut a = app();
        a.scroll = [3, 2, 1, 4, 0];
        assert!(a.full_titles); // default on
        a.handle_key(key(KeyCode::Char('w')));
        assert!(!a.full_titles);
        assert_eq!(a.scroll, [0; 5]);
        a.handle_key(key(KeyCode::Char('w')));
        assert!(a.full_titles);
    }

    #[test]
    fn r_requests_refresh_only_when_idle() {
        let mut a = app();
        assert_eq!(a.handle_key(key(KeyCode::Char('r'))), Intent::Refresh);
        a.is_fetching = true;
        assert_eq!(a.handle_key(key(KeyCode::Char('r'))), Intent::None);
    }

    #[test]
    fn renders_full_board_without_panic() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(200, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut a = app();
        a.is_fetching = true;
        terminal
            .draw(|frame| columns::render(frame, &mut a))
            .unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("tb-pr"));
        assert!(rendered.contains("Draft"));
        assert!(rendered.contains("Please review"));
        assert!(rendered.contains("fetching"));
        assert!(rendered.contains("[P-1234]"));
    }

    #[test]
    fn replace_state_clears_error_and_fetching() {
        let mut a = app();
        a.is_fetching = true;
        a.last_error = Some("boom".to_string());
        a.replace_state(sample_state());
        assert!(!a.is_fetching);
        assert!(a.last_error.is_none());
    }
}
