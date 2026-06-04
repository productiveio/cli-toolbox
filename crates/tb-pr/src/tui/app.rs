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

use crate::commands::util::{copy_to_clipboard, open_in_editor, open_url};
use crate::core::github::{GhClient, fetch_board_state, merge_with_previous};
use crate::core::model::{BoardState, Column, Notification, Pr, PrState};
use crate::core::worktree::WorktreeIndex;
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
    /// Open a local git-worktree path in the configured editor.
    OpenEditor(String),
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
    pub hide_drafts: bool,
    /// "Waiting on author" is a low-traffic column for most users, so it's
    /// collapsed out of the board by default and toggled back with `A`.
    pub hide_waiting_on_author: bool,
    pub is_fetching: bool,
    pub last_error: Option<String>,
    /// Transient non-error message (e.g. "copied <url>"). Rendered separately
    /// from `last_error` so success feedback doesn't show up as a red warning.
    pub status: Option<String>,
    pub tick_count: u64,
    productive_org_slug: String,
    /// Branch → local-worktree index, rebuilt on each refresh.
    worktrees: WorktreeIndex,
    /// Command used to open a worktree (e.g. `code`).
    editor: String,
}

impl App {
    pub fn new(
        state: BoardState,
        productive_org_slug: String,
        worktrees: WorktreeIndex,
        editor: String,
    ) -> Self {
        Self {
            state,
            focused: 0,
            selected: [0; COLUMNS_LEN],
            scroll: [0; COLUMNS_LEN],
            help_open: false,
            full_titles: true,
            hide_drafts: true,
            hide_waiting_on_author: true,
            is_fetching: false,
            last_error: None,
            status: None,
            tick_count: 0,
            productive_org_slug,
            worktrees,
            editor,
        }
    }

    pub fn board(&self) -> &BoardState {
        &self.state
    }

    /// Whether a column is currently collapsed out of the board view.
    fn column_hidden(&self, col: Column) -> bool {
        self.hide_waiting_on_author && col == Column::WaitingOnAuthor
    }

    /// `ORDER` positions of the columns currently shown, left-to-right. The
    /// position (not a 0..n slot) is what indexes `selected`/`scroll`, so it
    /// stays stable when a column is toggled on or off.
    pub fn visible_columns(&self) -> Vec<(usize, Column)> {
        ORDER
            .iter()
            .enumerate()
            .filter(|(_, c)| !self.column_hidden(**c))
            .map(|(i, c)| (i, *c))
            .collect()
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

    /// `column_prs(col)` with drafts filtered out when `hide_drafts` is on.
    /// Only `WaitingOnMe` / `WaitingOnAuthor` filter — `DraftMine` is left
    /// alone since drafts are the whole point of that column.
    pub fn visible_prs(&self, col: Column) -> Vec<&Pr> {
        let prs = self.column_prs(col);
        if self.hide_drafts && self.column_filters_drafts(col) {
            prs.iter().filter(|pr| pr.state != PrState::Draft).collect()
        } else {
            prs.iter().collect()
        }
    }

    pub fn hidden_draft_count(&self, col: Column) -> usize {
        if !self.hide_drafts || !self.column_filters_drafts(col) {
            return 0;
        }
        self.column_prs(col)
            .iter()
            .filter(|pr| pr.state == PrState::Draft)
            .count()
    }

    fn column_filters_drafts(&self, col: Column) -> bool {
        matches!(col, Column::WaitingOnMe | Column::WaitingOnAuthor)
    }

    pub fn column_notifications(&self) -> &[Notification] {
        &self.state.columns.notifications
    }

    /// Length of whichever list backs the column — PRs for the five
    /// canonical columns, notifications for Mentions.
    fn column_len(&self, col: Column) -> usize {
        match col {
            Column::Mentions => self.column_notifications().len(),
            _ => self.visible_prs(col).len(),
        }
    }

    fn focus_left(&mut self) {
        let vis = self.visible_columns();
        let pos = vis
            .iter()
            .position(|(i, _)| *i == self.focused)
            .unwrap_or(0);
        self.focused = vis[(pos + vis.len() - 1) % vis.len()].0;
    }

    fn focus_right(&mut self) {
        let vis = self.visible_columns();
        let pos = vis
            .iter()
            .position(|(i, _)| *i == self.focused)
            .unwrap_or(0);
        self.focused = vis[(pos + 1) % vis.len()].0;
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
        self.visible_prs(col)
            .get(self.selected[self.focused])
            .copied()
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

    /// Whether `pr`'s head branch has a local worktree in the configured roots.
    /// Drives the `⎇` marker on the card.
    pub fn has_worktree(&self, pr: &Pr) -> bool {
        pr.head_branch
            .as_deref()
            .is_some_and(|branch| self.worktrees.resolve(&pr.repo, branch).is_some())
    }

    /// Absolute worktree path for the selected PR, as an owned string. `None`
    /// when no PR is selected, the head branch is unknown, or no matching
    /// local checkout exists.
    fn selected_worktree_path(&self) -> Option<String> {
        let pr = self.selected_pr()?;
        let branch = pr.head_branch.as_deref()?;
        self.worktrees
            .resolve(&pr.repo, branch)
            .map(|p| p.display().to_string())
    }

    /// Set a "no local worktree" error tailored to the selected PR.
    fn flag_missing_worktree(&mut self) {
        let label = self
            .selected_pr()
            .map(|pr| format!("{}@{}", pr.repo, pr.head_branch.as_deref().unwrap_or("?")));
        self.last_error = Some(match label {
            Some(l) => format!("no local worktree for {l}"),
            None => "no PR selected".to_string(),
        });
    }

    /// Replace the worktree index after a background re-scan.
    pub fn set_worktrees(&mut self, worktrees: WorktreeIndex) {
        self.worktrees = worktrees;
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
            KeyCode::Char('f') => {
                self.full_titles = !self.full_titles;
                // Scroll offsets may now be wrong for the new card heights;
                // reset so the selected card re-anchors on next render.
                self.scroll = [0; COLUMNS_LEN];
                Intent::None
            }
            KeyCode::Char('w') => match self.selected_worktree_path() {
                Some(path) => Intent::CopyUrl(path),
                None => {
                    self.flag_missing_worktree();
                    Intent::None
                }
            },
            KeyCode::Char('e') => match self.selected_worktree_path() {
                Some(path) => Intent::OpenEditor(path),
                None => {
                    self.flag_missing_worktree();
                    Intent::None
                }
            },
            KeyCode::Char('D') => {
                self.hide_drafts = !self.hide_drafts;
                // Column counts change — clamp selections and reset scroll so
                // we don't index past the end of a now-shorter column.
                self.scroll = [0; COLUMNS_LEN];
                self.clamp_selections();
                Intent::None
            }
            KeyCode::Char('A') => {
                self.hide_waiting_on_author = !self.hide_waiting_on_author;
                // If we just collapsed the focused column, snap focus to the
                // nearest still-visible one (prefers the left neighbour).
                if self.column_hidden(self.focused_column()) {
                    self.focused = self
                        .visible_columns()
                        .iter()
                        .map(|(i, _)| *i)
                        .min_by_key(|i| i.abs_diff(self.focused))
                        .unwrap_or(0);
                }
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
    /// Directories scanned for local worktrees (`[worktrees].roots`).
    pub worktree_roots: Vec<String>,
    /// Editor command for the open-worktree shortcut (`[worktrees].editor`).
    pub editor: String,
}

#[derive(Debug)]
enum UiEvent {
    Key(KeyEvent),
    RefreshTick,
    FetchDone(std::result::Result<BoardState, String>),
    /// A background re-scan of the worktree roots finished.
    WorktreesScanned(WorktreeIndex),
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
    // Initial worktree scan so the cached board shows `⎇` markers before the
    // first background fetch even completes.
    let worktrees = WorktreeIndex::scan(&ctx.worktree_roots);
    let mut app = App::new(state, productive_slug, worktrees, ctx.editor.clone());
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
                Intent::OpenEditor(path) => match open_in_editor(&app.editor, &path) {
                    Ok(()) => app.set_status(format!("opened {path} in {}", app.editor)),
                    Err(e) => app.mark_error(format!("editor failed: {e}")),
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
            UiEvent::WorktreesScanned(index) => app.set_worktrees(index),
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

        // Re-scan worktrees alongside every refresh so a checkout created
        // mid-session is picked up by the next `r`. The scan shells out to
        // `git`, so run it off the async runtime.
        let roots = ctx.worktree_roots.clone();
        if let Ok(index) = tokio::task::spawn_blocking(move || WorktreeIndex::scan(&roots)).await {
            let _ = tx.send(UiEvent::WorktreesScanned(index));
        }
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
            head_branch: Some(format!("feature/{repo}-{number}")),
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
            column_issues: Vec::new(),
        }
    }

    fn app() -> App {
        // Index a worktree for the draft_mine PR (ai-agent #1) so the
        // worktree-shortcut tests have a match to resolve.
        let worktrees = WorktreeIndex::from_triples(&[(
            "feature/ai-agent-1",
            "ai-agent",
            "/Users/ivan/Code/worktrees/ai-agent-1",
        )]);
        let mut a = App::new(
            sample_state(),
            "109-productive".to_string(),
            worktrees,
            "code".to_string(),
        );
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
    fn f_toggles_full_titles_and_resets_scroll() {
        let mut a = app();
        a.scroll = [3, 2, 1, 4, 0, 0];
        assert!(a.full_titles); // default on
        a.handle_key(key(KeyCode::Char('f')));
        assert!(!a.full_titles);
        assert_eq!(a.scroll, [0; COLUMNS_LEN]);
        a.handle_key(key(KeyCode::Char('f')));
        assert!(a.full_titles);
    }

    #[test]
    fn waiting_on_author_hidden_by_default_and_toggled_with_a() {
        let mut a = app();
        let author_idx = ORDER
            .iter()
            .position(|c| *c == Column::WaitingOnAuthor)
            .unwrap();
        let on_me_idx = ORDER
            .iter()
            .position(|c| *c == Column::WaitingOnMe)
            .unwrap();

        // Hidden by default — absent from the layout and skipped by navigation.
        assert!(a.hide_waiting_on_author);
        assert!(!a.visible_columns().iter().any(|(i, _)| *i == author_idx));
        a.focused = on_me_idx;
        a.handle_key(key(KeyCode::Right));
        assert_eq!(a.focused_column(), Column::Mentions);

        // `A` reveals it; it's now in the layout and navigable.
        a.handle_key(key(KeyCode::Char('A')));
        assert!(!a.hide_waiting_on_author);
        assert!(a.visible_columns().iter().any(|(i, _)| *i == author_idx));
        a.focused = on_me_idx;
        a.handle_key(key(KeyCode::Right));
        assert_eq!(a.focused_column(), Column::WaitingOnAuthor);
    }

    #[test]
    fn hiding_focused_author_column_snaps_to_left_neighbour() {
        let mut a = app();
        a.handle_key(key(KeyCode::Char('A'))); // reveal
        a.focused = ORDER
            .iter()
            .position(|c| *c == Column::WaitingOnAuthor)
            .unwrap();
        a.handle_key(key(KeyCode::Char('A'))); // hide again while focused on it
        assert!(a.hide_waiting_on_author);
        assert_eq!(a.focused_column(), Column::WaitingOnMe);
    }

    #[test]
    fn w_copies_worktree_path_when_present() {
        let mut a = app(); // draft_mine PR (ai-agent #1) has an indexed worktree
        match a.handle_key(key(KeyCode::Char('w'))) {
            Intent::CopyUrl(path) => {
                assert_eq!(path, "/Users/ivan/Code/worktrees/ai-agent-1");
            }
            other => panic!("expected CopyUrl(path), got {other:?}"),
        }
        assert!(a.has_worktree(a.selected_pr().unwrap()));
    }

    #[test]
    fn e_opens_editor_at_worktree() {
        let mut a = app();
        match a.handle_key(key(KeyCode::Char('e'))) {
            Intent::OpenEditor(path) => {
                assert_eq!(path, "/Users/ivan/Code/worktrees/ai-agent-1");
            }
            other => panic!("expected OpenEditor(path), got {other:?}"),
        }
    }

    #[test]
    fn worktree_shortcuts_flag_error_when_absent() {
        let mut a = app();
        // review_mine PR (api #2) has no indexed worktree.
        a.focused = ORDER.iter().position(|c| *c == Column::ReviewMine).unwrap();
        assert_eq!(a.handle_key(key(KeyCode::Char('w'))), Intent::None);
        assert_eq!(
            a.last_error.as_deref(),
            Some("no local worktree for api@feature/api-2")
        );
        assert_eq!(a.handle_key(key(KeyCode::Char('e'))), Intent::None);
        assert!(!a.has_worktree(a.selected_pr().unwrap()));
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
    fn hide_drafts_default_filters_drafts_from_waiting_columns_only() {
        let mut state = sample_state();
        let mut draft_pr = sample_pr("api", 9, "WIP review");
        draft_pr.state = PrState::Draft;
        state.columns.waiting_on_me.push(draft_pr);
        let mut draft_author = sample_pr("api", 10, "WIP author");
        draft_author.state = PrState::Draft;
        state.columns.waiting_on_author.push(draft_author);

        let mut a = App::new(
            state,
            "109-productive".to_string(),
            WorktreeIndex::default(),
            "code".to_string(),
        );
        a.clamp_selections();

        // Default: drafts hidden in non-mine review columns, but not in DraftMine.
        assert!(a.hide_drafts);
        assert_eq!(a.visible_prs(Column::WaitingOnMe).len(), 1);
        assert_eq!(a.hidden_draft_count(Column::WaitingOnMe), 1);
        assert_eq!(a.visible_prs(Column::WaitingOnAuthor).len(), 0);
        assert_eq!(a.hidden_draft_count(Column::WaitingOnAuthor), 1);
        // DraftMine is never filtered — its whole point is drafts.
        assert_eq!(a.visible_prs(Column::DraftMine).len(), 1);
        assert_eq!(a.hidden_draft_count(Column::DraftMine), 0);
    }

    #[test]
    fn shift_d_toggles_draft_filter_and_resets_scroll() {
        let mut a = app();
        a.scroll = [3, 2, 1, 4, 0, 0];
        assert!(a.hide_drafts); // default on

        a.handle_key(key(KeyCode::Char('D')));
        assert!(!a.hide_drafts);
        assert_eq!(a.scroll, [0; COLUMNS_LEN]);

        a.handle_key(key(KeyCode::Char('D')));
        assert!(a.hide_drafts);
    }

    #[test]
    fn toggling_filter_clamps_selection_to_visible_count() {
        let mut state = sample_state();
        // waiting_on_me has 1 ready PR + 1 draft. With filter on, only the
        // ready PR is visible; selecting index 1 would be out of bounds.
        let mut draft_pr = sample_pr("api", 9, "WIP");
        draft_pr.state = PrState::Draft;
        state.columns.waiting_on_me.push(draft_pr);
        let mut a = App::new(
            state,
            "109-productive".to_string(),
            WorktreeIndex::default(),
            "code".to_string(),
        );
        a.clamp_selections();

        a.focused = ORDER
            .iter()
            .position(|c| *c == Column::WaitingOnMe)
            .unwrap();
        // Disable filter via key handler so both PRs are visible, point at the
        // draft (index 1), then toggle the filter back on. clamp_selections
        // must walk the selection back to the last visible index.
        a.handle_key(key(KeyCode::Char('D')));
        assert!(!a.hide_drafts);
        a.selected[a.focused] = 1;
        a.handle_key(key(KeyCode::Char('D')));
        assert!(a.hide_drafts);
        assert_eq!(a.selected[a.focused], 0);
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
