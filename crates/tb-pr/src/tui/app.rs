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
use crate::core::github::{GhClient, fetch_board_state, merge_with_previous};
use crate::core::model::{BoardState, Column, Notification, Pr};
use crate::error::{Error, Result};
use crate::tui::columns;

/// Six columns in left-to-right kanban order. Mentions lives on the right —
/// it's email-replacement inbox, not a PR status column.
const ORDER: [Column; 6] = [
    Column::DraftMine,
    Column::ReviewMine,
    Column::ReadyToMergeMine,
    Column::WaitingOnMe,
    Column::WaitingOnAuthor,
    Column::Mentions,
];
const COLUMNS_LEN: usize = ORDER.len();

/// What the app wants the event loop to do after handling a key.
#[derive(Debug, PartialEq, Eq)]
pub enum Intent {
    None,
    Quit,
    Refresh,
    OpenUrl(String),
    CopyUrl(String),
    /// Resolve the latest comment on a PR → open → mark the thread as read.
    /// `fallback_pr_url` is used when neither comment feed has anything.
    OpenNotification {
        thread_id: String,
        owner: String,
        repo: String,
        pr_number: u64,
        fallback_pr_url: String,
    },
    /// Server-side mark-all-as-read + clear the local list.
    MarkAllNotificationsRead,
}

pub struct App {
    state: BoardState,
    /// Index into `ORDER` — which column is focused.
    pub focused: usize,
    /// Selected card index per column.
    pub selected: [usize; COLUMNS_LEN],
    /// First visible card index per column (for scrolling).
    pub scroll: [usize; COLUMNS_LEN],
    pub help_open: bool,
    pub full_titles: bool,
    pub is_fetching: bool,
    pub last_error: Option<String>,
    /// Transient non-error message (e.g. "copied <url>"). Rendered separately
    /// from `last_error` so success feedback doesn't show up as a red warning.
    pub status: Option<String>,
    pub tick_count: u64,
    productive_org_slug: String,
}

impl App {
    pub fn new(state: BoardState, productive_org_slug: String) -> Self {
        Self {
            state,
            focused: 0,
            selected: [0; COLUMNS_LEN],
            scroll: [0; COLUMNS_LEN],
            help_open: false,
            full_titles: true,
            is_fetching: false,
            last_error: None,
            status: None,
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
            // Notifications aren't Prs — callers hitting Mentions should
            // use column_notifications() instead. Returning empty here lets
            // shared navigation code stay generic over item count.
            Column::Mentions => &[],
        }
    }

    pub fn column_notifications(&self) -> &[Notification] {
        &self.state.columns.notifications
    }

    /// Length of whichever list backs the column — PRs for the five
    /// canonical columns, notifications for Mentions.
    fn column_len(&self, col: Column) -> usize {
        match col {
            Column::Mentions => self.column_notifications().len(),
            _ => self.column_prs(col).len(),
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
        let len = self.column_len(col);
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
            let len = self.column_len(*col);
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
        if col == Column::Mentions {
            return None;
        }
        self.column_prs(col).get(self.selected[self.focused])
    }

    fn selected_notification(&self) -> Option<&Notification> {
        if self.focused_column() != Column::Mentions {
            return None;
        }
        self.column_notifications().get(self.selected[self.focused])
    }

    /// A URL to copy / open for whichever kind of card is focused — a PR's
    /// URL, a notification's PR URL, or `None` if the focused column is empty.
    fn selected_url(&self) -> Option<String> {
        self.selected_pr()
            .map(|pr| pr.url.clone())
            .or_else(|| self.selected_notification().map(|n| n.pr_url.clone()))
    }

    /// Remove a notification (after the user opens + the server confirms the
    /// thread is read). Called from the event loop on FetchDone-style events.
    pub fn remove_notification(&mut self, thread_id: &str) {
        self.state
            .columns
            .notifications
            .retain(|n| n.thread_id != thread_id);
        self.clamp_selections();
    }

    pub fn clear_notifications(&mut self) {
        self.state.columns.notifications.clear();
        self.clamp_selections();
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
        self.status = None;
        self.is_fetching = false;
        self.clamp_selections();
    }

    pub fn mark_error(&mut self, err: String) {
        self.last_error = Some(err);
        self.status = None;
        self.is_fetching = false;
    }

    pub fn set_status(&mut self, msg: String) {
        self.status = Some(msg);
        self.last_error = None;
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
            KeyCode::Enter | KeyCode::Char('d') => {
                if let Some(n) = self.selected_notification() {
                    return Intent::OpenNotification {
                        thread_id: n.thread_id.clone(),
                        owner: n.owner.clone(),
                        repo: n.repo.clone(),
                        pr_number: n.pr_number,
                        fallback_pr_url: n.pr_url.clone(),
                    };
                }
                self.selected_pr()
                    .map(|pr| Intent::OpenUrl(pr.url.clone()))
                    .unwrap_or(Intent::None)
            }
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
                .selected_url()
                .map(Intent::CopyUrl)
                .unwrap_or(Intent::None),
            KeyCode::Char('r') => {
                if !self.is_fetching {
                    Intent::Refresh
                } else {
                    Intent::None
                }
            }
            KeyCode::Char('m') => {
                if self.column_notifications().is_empty() {
                    Intent::None
                } else {
                    Intent::MarkAllNotificationsRead
                }
            }
            KeyCode::Char('w') => {
                self.full_titles = !self.full_titles;
                // Scroll offsets may now be wrong for the new card heights;
                // reset so the selected card re-anchors on next render.
                self.scroll = [0; COLUMNS_LEN];
                Intent::None
            }
            _ => Intent::None,
        }
    }
}

/// Config passed in from the CLI — everything the background fetcher needs
/// plus the auto-refresh cadence.
#[derive(Clone)]
pub struct FetchCtx {
    pub org: String,
    pub productive_org_slug: String,
    pub username_override: String,
    pub refresh_interval: Duration,
}

#[derive(Debug)]
enum UiEvent {
    Key(KeyEvent),
    RefreshTick,
    FetchDone(std::result::Result<BoardState, String>),
    AnimTick,
    /// Background task finished opening + marking a single notification.
    /// `Ok(thread_id)` → remove it; `Err(msg)` → surface the error and keep it.
    NotificationOpened(std::result::Result<String, String>),
    /// Server accepted PUT /notifications — drop all local notifications.
    NotificationsAllRead(std::result::Result<(), String>),
}

pub async fn run(state: BoardState, ctx: FetchCtx, needs_refresh: bool) -> Result<()> {
    // The guard's Drop impl restores the terminal on every exit path,
    // including panics inside `event_loop`. Without it a crash left the
    // user's shell in raw mode + alt-screen — unusable.
    let _guard = RawModeGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout())).map_err(Error::Io)?;
    let productive_slug = ctx.productive_org_slug.clone();
    let mut app = App::new(state, productive_slug);
    app.clamp_selections();

    let result = event_loop(&mut terminal, &mut app, ctx, needs_refresh).await;
    let _ = terminal.show_cursor();
    result
}

async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ctx: FetchCtx,
    needs_refresh: bool,
) -> Result<()> {
    let (tx, mut rx) = unbounded_channel::<UiEvent>();
    let ctx = Arc::new(ctx);

    // Keyboard pump — blocking crossterm on a dedicated thread.
    let tx_kb = tx.clone();
    std::thread::spawn(move || keyboard_pump(tx_kb));

    // Auto-refresh timer. Interval comes from config (`refresh.interval_minutes`).
    // Guard against misconfigured zero/very-small values that would hammer the API.
    let refresh_interval = ctx.refresh_interval.max(Duration::from_secs(30));
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(refresh_interval);
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

    // Draw once up front so the user sees *something* immediately — even if
    // the cache was empty, the empty columns + header spinner appear instantly
    // instead of the process looking hung for 5 seconds.
    terminal
        .draw(|frame| columns::render(frame, app))
        .map_err(Error::Io)?;

    // Kick off the initial fetch if the cache was missing or stale. Runs on
    // a tokio task; UI stays responsive. Spinner in the header shows progress.
    if needs_refresh {
        spawn_fetch(&tx, &ctx, app);
        terminal
            .draw(|frame| columns::render(frame, app))
            .map_err(Error::Io)?;
    }

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
                    Ok(()) => app.set_status(format!("copied {url}")),
                    Err(e) => app.mark_error(format!("copy failed: {e}")),
                },
                Intent::OpenNotification {
                    thread_id,
                    owner,
                    repo,
                    pr_number,
                    fallback_pr_url,
                } => {
                    spawn_open_notification(
                        &tx,
                        thread_id,
                        owner,
                        repo,
                        pr_number,
                        fallback_pr_url,
                    );
                }
                Intent::MarkAllNotificationsRead => {
                    spawn_mark_all_read(&tx);
                    app.set_status("marking all notifications read…".to_string());
                }
                Intent::None => {}
            },
            UiEvent::RefreshTick => {
                if !app.is_fetching {
                    spawn_fetch(&tx, &ctx, app);
                }
            }
            UiEvent::FetchDone(Ok(state)) => app.replace_state(state),
            UiEvent::FetchDone(Err(msg)) => app.mark_error(msg),
            UiEvent::NotificationOpened(Ok(thread_id)) => {
                app.remove_notification(&thread_id);
                app.set_status("marked read".to_string());
            }
            UiEvent::NotificationOpened(Err(msg)) => app.mark_error(msg),
            UiEvent::NotificationsAllRead(Ok(())) => {
                app.clear_notifications();
                app.set_status("all notifications marked read".to_string());
            }
            UiEvent::NotificationsAllRead(Err(msg)) => {
                app.mark_error(format!("mark-all-read failed: {msg}"));
            }
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

/// Open a notification in the browser and mark its thread as read.
/// Three steps, each independently fallible:
///   1. resolve the PR's latest comment → `html_url` (falls back to PR URL)
///   2. open that URL in the browser
///   3. PATCH the thread as read
fn spawn_open_notification(
    tx: &UnboundedSender<UiEvent>,
    thread_id: String,
    owner: String,
    repo: String,
    pr_number: u64,
    fallback_pr_url: String,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result: std::result::Result<String, String> = async {
            let client = GhClient::new().map_err(|e| e.to_string())?;
            let url = client
                .latest_comment_html_url(&owner, &repo, pr_number)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| fallback_pr_url.clone());
            open_url(&url).map_err(|e| e.to_string())?;
            client
                .mark_thread_read(&thread_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(thread_id)
        }
        .await;
        let _ = tx.send(UiEvent::NotificationOpened(result));
    });
}

fn spawn_mark_all_read(tx: &UnboundedSender<UiEvent>) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = async {
            let client = GhClient::new().map_err(|e| e.to_string())?;
            client
                .mark_all_notifications_read()
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        let _ = tx.send(UiEvent::NotificationsAllRead(result));
    });
}

fn spawn_fetch(tx: &UnboundedSender<UiEvent>, ctx: &Arc<FetchCtx>, app: &mut App) {
    app.is_fetching = true;
    app.last_error = None;
    // Snapshot the current in-memory state to use as fallback if any
    // search column comes back empty (GitHub search-index degradation).
    let prev = app.state.clone();
    let tx = tx.clone();
    let ctx = ctx.clone();
    tokio::spawn(async move {
        let fresh = match GhClient::new() {
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
        let result = fresh.map(|state| merge_with_previous(state, Some(prev)));
        // Persist the successful fetch to the cache before handing off to the
        // UI. Without this, relaunching the app would show the stale count
        // again and re-fetch on every open. Cache write errors are swallowed
        // — stale cache is a UX nuisance, not a correctness issue worth
        // crashing the session over.
        if let Ok(state) = &result
            && let Ok(cache) = crate::core::cache::BoardCache::new()
        {
            let _ = cache.save_board(state);
        }
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

/// Enters raw mode + alternate screen on construction and leaves them on
/// drop. Placing the teardown in Drop means any panic path still restores
/// the terminal — crossterm's enable_raw_mode / EnterAlternateScreen leave
/// the shell in a corrupted state otherwise.
struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().map_err(Error::Io)?;
        execute!(io::stdout(), EnterAlternateScreen).map_err(Error::Io)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Ignore errors on shutdown — there's nothing useful we can do, and
        // Drop cannot return a Result.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        BoardState, ColumnsData, NotificationReason, Pr, PrState, RottingBucket, SizeBucket,
    };
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
            check_state: None,
        }
    }

    fn sample_notification(repo: &str, pr_number: u64, title: &str) -> Notification {
        Notification {
            thread_id: format!("thread-{pr_number}"),
            reason: NotificationReason::Mention,
            owner: "productiveio".to_string(),
            repo: repo.to_string(),
            pr_number,
            pr_title: title.to_string(),
            pr_url: format!("https://github.com/productiveio/{repo}/pull/{pr_number}"),
            updated_at: Utc::now(),
            age_days: 0.5,
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
                notifications: vec![sample_notification("frontend", 3, "Please review")],
            },
            degraded_columns: Vec::new(),
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
        assert_eq!(a.focused, COLUMNS_LEN - 1);
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
        a.scroll = [3, 2, 1, 4, 0, 0];
        assert!(a.full_titles); // default on
        a.handle_key(key(KeyCode::Char('w')));
        assert!(!a.full_titles);
        assert_eq!(a.scroll, [0; COLUMNS_LEN]);
        a.handle_key(key(KeyCode::Char('w')));
        assert!(a.full_titles);
    }

    #[test]
    fn enter_on_mentions_returns_open_notification_intent() {
        let mut a = app();
        a.focused = ORDER.iter().position(|c| *c == Column::Mentions).unwrap();
        match a.handle_key(key(KeyCode::Enter)) {
            Intent::OpenNotification {
                thread_id,
                owner,
                repo,
                pr_number,
                fallback_pr_url,
            } => {
                assert_eq!(thread_id, "thread-3");
                assert_eq!(owner, "productiveio");
                assert_eq!(repo, "frontend");
                assert_eq!(pr_number, 3);
                assert!(fallback_pr_url.contains("frontend/pull/3"));
            }
            other => panic!("expected OpenNotification, got {other:?}"),
        }
    }

    #[test]
    fn m_marks_all_read_only_when_inbox_nonempty() {
        let mut a = app();
        assert_eq!(
            a.handle_key(key(KeyCode::Char('m'))),
            Intent::MarkAllNotificationsRead
        );
        a.clear_notifications();
        assert_eq!(a.handle_key(key(KeyCode::Char('m'))), Intent::None);
    }

    #[test]
    fn remove_notification_clamps_selection() {
        let mut a = app();
        let mentions_idx = ORDER.iter().position(|c| *c == Column::Mentions).unwrap();
        a.focused = mentions_idx;
        a.selected[mentions_idx] = 0;
        a.remove_notification("thread-3");
        assert!(a.column_notifications().is_empty());
        assert_eq!(a.selected[mentions_idx], 0);
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
        a.status = Some("copied …".to_string());
        a.replace_state(sample_state());
        assert!(!a.is_fetching);
        assert!(a.last_error.is_none());
        assert!(a.status.is_none());
    }

    #[test]
    fn status_and_error_are_mutually_exclusive() {
        let mut a = app();
        a.set_status("copied foo".into());
        assert_eq!(a.status.as_deref(), Some("copied foo"));
        assert!(a.last_error.is_none());

        a.mark_error("boom".into());
        assert_eq!(a.last_error.as_deref(), Some("boom"));
        assert!(a.status.is_none());
    }
}
