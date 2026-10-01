//! TUI application state and event loop.

use anyhow::Context;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::time::{Duration, Instant};

use super::backend::{DashboardSnapshot, WorkspaceBackend};
use super::operations::{OperationTracker, TrackedOperation};
use super::palette::{CommandAction, PaletteState};
use super::screens::{self, ScreenId, ScreenState, SCREENS};
use super::shell;

/// How long a Dashboard snapshot stays fresh before the bounded
/// refresh recomputes it (spec section 7: no expensive continuous
/// polling — the git subprocess calls run at most once per TTL).
const DASHBOARD_TTL: Duration = Duration::from_secs(2);

/// Shared TTL for the premium read-only view caches (agents, tasks,
/// changes, commits, audit). Per-view 'r' refresh and mutations
/// invalidate immediately; this bound keeps renders cheap.
const VIEW_TTL: Duration = Duration::from_secs(3);

/// A concrete action the user asked the UI to perform. Destructive
/// actions wait for modal confirmation before `execute` runs them.
#[derive(Debug, Clone)]
pub enum ActionKind {
    DeleteProject(String),
    DeletePath(String),
    /// Discard unsaved editor changes for the given path.
    DiscardChanges(String),
    /// Start a registered agent (service-owned lifecycle).
    AgentStart(String),
    /// Stop a running agent (terminal; service-owned lifecycle).
    AgentStop(String),
}

impl ActionKind {
    pub fn destructive(&self) -> bool {
        matches!(
            self,
            ActionKind::DeleteProject(_)
                | ActionKind::DeletePath(_)
                | ActionKind::DiscardChanges(_)
        )
    }

    fn describe(&self) -> String {
        match self {
            ActionKind::DeleteProject(name) => format!("delete project {name:?} and all its files"),
            ActionKind::DeletePath(path) => format!("delete {path:?}"),
            ActionKind::DiscardChanges(path) => format!("discard unsaved changes to {path:?}"),
            ActionKind::AgentStart(id) => format!("start agent {id}"),
            ActionKind::AgentStop(id) => format!("stop agent {id}"),
        }
    }
}

/// One pending UI action awaiting (non-destructive) or bypassing
/// (non-destructive) modal confirmation.
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub kind: ActionKind,
}

impl From<ActionKind> for PendingAction {
    fn from(kind: ActionKind) -> Self {
        Self { kind }
    }
}

/// TTL cache for one premium view. Mutation-driven invalidation clears
/// it; manual 'r' refresh forces recompute; renders within the TTL
/// serve the cached copy without touching the backend.
struct ViewCache<T> {
    data: Option<T>,
    fresh_until: Option<Instant>,
}

impl<T> ViewCache<T> {
    fn new() -> Self {
        Self {
            data: None,
            fresh_until: None,
        }
    }

    fn invalidate(&mut self) {
        self.fresh_until = None;
    }

    fn get_or_reload(&mut self, reload: impl FnOnce() -> anyhow::Result<T>) -> Option<&T> {
        let fresh = self.fresh_until.is_some_and(|t| Instant::now() < t);
        if !fresh {
            match reload() {
                Ok(data) => {
                    self.data = Some(data);
                    self.fresh_until = Some(Instant::now() + VIEW_TTL);
                }
                Err(_) => {
                    // Errors are not cached: the next render retries the
                    // backend so a recovered backend heals without 'r'.
                    self.fresh_until = None;
                }
            }
        }
        self.data.as_ref()
    }
}

impl<T> Default for ViewCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level application state shared by all screens.
pub struct App<B: WorkspaceBackend> {
    pub backend: B,
    screen: ScreenId,
    /// Screens below this one in the navigation stack, for Escape-based pop.
    stack: Vec<ScreenId>,
    /// Modal confirmation state.
    pub confirm: Option<PendingAction>,
    /// Last error to display; dismissed by the user.
    pub error: Option<String>,
    /// Status message for the footer bar.
    pub message: Option<String>,
    /// Per-screen UI state (selections, listings).
    pub ui: ScreenState,
    /// Command palette overlay state.
    pub palette: PaletteState,
    /// Operation progress/notifications (bounded).
    pub ops: OperationTracker,
    /// Bounded-refresh cache for the Dashboard (spec section 7).
    dashboard_cache: Option<DashboardSnapshot>,
    dashboard_fresh_until: Option<Instant>,
    /// Premium-view caches: agents / sessions / tasks / changes /
    /// commits / audit.
    agents_cache: ViewCache<Vec<super::backend::AgentView>>,
    sessions_cache: ViewCache<Vec<super::backend::SessionView>>,
    tasks_cache: ViewCache<Vec<super::backend::TaskView>>,
    changes_cache: ViewCache<Vec<super::backend::ChangedFile>>,
    commits_cache: ViewCache<Vec<super::backend::CommitView>>,
    audit_cache: ViewCache<Vec<super::backend::AuditRow>>,
    quit: bool,
}

impl<B: WorkspaceBackend> App<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            screen: ScreenId::Dashboard,
            stack: Vec::new(),
            confirm: None,
            error: None,
            message: None,
            ui: ScreenState::default(),
            palette: PaletteState::default(),
            ops: OperationTracker::default(),
            dashboard_cache: None,
            dashboard_fresh_until: None,
            agents_cache: ViewCache::new(),
            sessions_cache: ViewCache::new(),
            tasks_cache: ViewCache::new(),
            changes_cache: ViewCache::new(),
            commits_cache: ViewCache::new(),
            audit_cache: ViewCache::new(),
            quit: false,
        }
    }

    pub fn screen(&self) -> ScreenId {
        self.screen
    }

    pub fn quit(&self) -> bool {
        self.quit
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    /// Headline operation for the footer status line, if any.
    pub fn pending_operation(&self) -> Option<&TrackedOperation> {
        self.ops.headline()
    }

    // ----- View caches (bounded refresh + mutation invalidation) -----

    pub fn cached_agents(&mut self) -> Option<&Vec<super::backend::AgentView>> {
        self.agents_cache
            .get_or_reload(|| self.backend.list_agents())
    }

    pub fn cached_sessions(&mut self) -> Option<&Vec<super::backend::SessionView>> {
        self.sessions_cache
            .get_or_reload(|| self.backend.list_sessions(None))
    }

    pub fn cached_tasks(&mut self) -> Option<&Vec<super::backend::TaskView>> {
        self.tasks_cache.get_or_reload(|| self.backend.list_tasks())
    }

    pub fn cached_changes(&mut self) -> Option<&Vec<super::backend::ChangedFile>> {
        self.changes_cache
            .get_or_reload(|| self.backend.list_changed_files())
    }

    pub fn cached_commits(&mut self) -> Option<&Vec<super::backend::CommitView>> {
        self.commits_cache
            .get_or_reload(|| self.backend.recent_commits(20))
    }

    pub fn cached_audit(&mut self) -> Option<&Vec<super::backend::AuditRow>> {
        self.audit_cache
            .get_or_reload(|| self.backend.list_audit(200))
    }

    /// Invalidates every data view (scope change, mutation, or manual
    /// refresh). Cheap: only TTLs are cleared.
    pub fn invalidate_views(&mut self) {
        self.dashboard_fresh_until = None;
        self.agents_cache.invalidate();
        self.sessions_cache.invalidate();
        self.tasks_cache.invalidate();
        self.changes_cache.invalidate();
        self.commits_cache.invalidate();
        self.audit_cache.invalidate();
    }

    /// Manual 'r' refresh for the active view.
    pub fn refresh_active_view(&mut self) {
        self.invalidate_views();
        self.set_message("refreshed");
    }

    /// Resets per-view selection/cursor state after a scope change
    /// (Prompt 30: stale-selection invalidation). Selections that point
    /// at another project's rows are never carried across.
    pub fn reset_view_state(&mut self) {
        self.ui.projects.select(Some(0));
        self.ui.files.select(Some(0));
        self.ui.git.select(Some(0));
        self.ui.files_ui.cwd.clear();
        self.ui.files_ui.search_results.clear();
        self.ui.files_ui.show_search = false;
        self.ui.files_ui.input.clear();
        self.ui.files_ui.input_mode = None;
        self.ui.audit.select(Some(0));
    }

    // ----- Dashboard (pre-existing bounded refresh) -----

    pub fn dashboard_cached(&mut self) -> DashboardSnapshot {
        let fresh = self
            .dashboard_fresh_until
            .is_some_and(|t| Instant::now() < t);
        if !fresh {
            if let Ok(snap) = self.backend.dashboard() {
                self.dashboard_cache = Some(snap);
                self.dashboard_fresh_until = Some(Instant::now() + DASHBOARD_TTL);
            }
        }
        self.dashboard_cache.clone().unwrap_or_default()
    }

    pub fn invalidate_dashboard(&mut self) {
        self.dashboard_fresh_until = None;
    }

    // ----- Actions -----

    /// Requests an action. Destructive actions are held until the user
    /// confirms the modal; non-destructive ones execute immediately.
    pub fn request_action(&mut self, kind: ActionKind) {
        let action = PendingAction { kind };
        if action.kind.destructive() {
            self.ops.track(action.kind.describe());
            self.ops.to_authorizing();
            self.confirm = Some(action);
        } else {
            self.execute(action.kind);
        }
    }

    /// Confirms the pending destructive action and executes it.
    pub fn confirm_pending(&mut self) {
        if let Some(action) = self.confirm.take() {
            self.execute(action.kind);
        }
    }

    pub fn cancel_pending(&mut self) {
        self.confirm = None;
    }

    /// Runs an action against the backend and records the outcome in
    /// the operation tracker + footer. Unknown-result classification
    /// (remote timeouts) stays visible until reconciled.
    fn execute(&mut self, kind: ActionKind) {
        self.ops.track(kind.describe());
        self.ops.to_executing();

        // UI-local: reload the editor buffer through the backend and
        // stash it for the Editor screen to adopt.
        if let ActionKind::DiscardChanges(path) = &kind {
            match self.backend.read_file(path) {
                Ok(fresh) => {
                    self.ui.reload_content = Some((path.clone(), fresh));
                    self.ops.to_confirmed();
                    self.set_message(format!("done: {}", kind.describe()));
                }
                Err(e) => {
                    self.ops.to_failed(format!("{e:#}"));
                    self.set_error(format!("{}: {e:#}", kind.describe()));
                }
            }
            return;
        }

        let result: anyhow::Result<()> = match &kind {
            ActionKind::DeleteProject(name) => self
                .backend
                .delete_project(name)
                .with_context(|| format!("delete project {name}")),
            ActionKind::DeletePath(path) => self
                .backend
                .delete_path(path)
                .with_context(|| format!("delete {path}")),
            ActionKind::AgentStart(id) => self.backend.agent_start(id),
            ActionKind::AgentStop(id) => self.backend.agent_stop(id),
            ActionKind::DiscardChanges(_) => unreachable!("handled above"),
        };
        match result {
            Ok(()) => {
                self.ops.to_confirmed();
                // Workspace shape changed; every data view must not
                // serve stale state.
                self.invalidate_views();
                if matches!(kind, ActionKind::AgentStart(_) | ActionKind::AgentStop(_)) {
                    self.agents_cache.invalidate();
                    self.sessions_cache.invalidate();
                }
                self.set_message(format!("done: {}", kind.describe()));
            }
            Err(e) => {
                let text = format!("{e:#}");
                // Ambiguous remote timeouts are Unknown, not Failed:
                // the effect may have happened; reconcile before retry.
                if text.contains("timed out") || text.contains("timeout") {
                    self.ops.to_unknown(text.clone());
                } else {
                    self.ops.to_failed(text.clone());
                }
                self.set_error(format!("{}: {text}", kind.describe()));
            }
        }
    }

    // ----- Palette execution (same paths as key handlers) -----

    pub fn run_palette_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::Navigate(id) => self.goto(id),
            CommandAction::Refresh => self.refresh_active_view(),
            CommandAction::Help => self.goto_fresh(ScreenId::Help),
            CommandAction::Quit => {
                self.quit = true;
            }
            CommandAction::OpenProject(name) => {
                self.ops.track(format!("open project {name}"));
                self.ops.to_executing();
                match self.backend.open_project(&name) {
                    Ok(()) => {
                        self.ops.to_confirmed();
                        // Scope changed: no stale selections may survive.
                        self.reset_view_state();
                        self.invalidate_views();
                        self.set_message(format!("opened project {name}"));
                    }
                    Err(e) => {
                        self.ops.to_failed(format!("{e:#}"));
                        self.set_error(format!("open project {name}: {e:#}"));
                    }
                }
            }
            CommandAction::Request(kind) => self.request_action(kind),
        }
    }

    // ----- Navigation -----

    pub fn goto(&mut self, screen: ScreenId) {
        if screen == self.screen {
            return;
        }
        self.stack.push(self.screen);
        self.screen = screen;
        self.message = None;
    }

    /// Moves to the given screen without remembering the current one
    /// (used by the tab ring so Escape doesn't zig-zag).
    pub fn goto_fresh(&mut self, screen: ScreenId) {
        if screen != self.screen {
            self.screen = screen;
        }
        self.message = None;
    }

    pub fn back(&mut self) {
        if let Some(previous) = self.stack.pop() {
            self.screen = previous;
        } else {
            self.quit = true;
        }
    }

    pub fn next_screen(&mut self) {
        let index = SCREENS
            .iter()
            .position(|s| s.id == self.screen)
            .unwrap_or(0);
        let next = SCREENS[(index + 1) % SCREENS.len()].id;
        self.goto_fresh(next);
    }

    pub fn prev_screen(&mut self) {
        let index = SCREENS
            .iter()
            .position(|s| s.id == self.screen)
            .unwrap_or(0);
        let prev = SCREENS[(index + SCREENS.len() - 1) % SCREENS.len()].id;
        self.goto_fresh(prev);
    }
}

/// Drives the UI until the user quits. Returns when the terminal is still
/// in raw mode; callers own `ratatui::restore()`.
pub fn run<B: WorkspaceBackend>(
    terminal: &mut ratatui::DefaultTerminal,
    backend: B,
) -> anyhow::Result<()> {
    let mut app = App::new(backend);
    while !app.quit() {
        terminal.draw(|frame| draw(frame, &mut app))?;
        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key);
                }
            }
        }
    }
    Ok(())
}

/// Global key dispatch. Priority: modal confirmation > palette overlay >
/// screen input capture > global keys > active screen.
fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    // Destructive confirmation intercepts everything so no second
    // action can be triggered while a confirmation is pending.
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => app.confirm_pending(),
            KeyCode::Char('n') | KeyCode::Esc => app.cancel_pending(),
            _ => {}
        }
        return;
    }

    // Command palette overlay owns the keyboard while open.
    if app.palette.open {
        match key.code {
            KeyCode::Esc => app.palette.close(),
            KeyCode::Backspace => {
                app.palette.query.pop();
            }
            KeyCode::Up => {
                if app.palette.selected > 0 {
                    app.palette.selected -= 1;
                }
            }
            KeyCode::Down => {
                let count = super::palette::matches(app).len();
                if app.palette.selected + 1 < count {
                    app.palette.selected += 1;
                }
            }
            KeyCode::Enter => {
                let action = super::palette::matches(app)
                    .into_iter()
                    .nth(app.palette.selected)
                    .and_then(|cmd| cmd.enabled.then_some(cmd.action));
                app.palette.close();
                if let Some(action) = action {
                    app.run_palette_command(action);
                } else {
                    app.set_message("command unavailable");
                }
            }
            KeyCode::Char(c) => app.palette.query.push(c),
            _ => {}
        }
        return;
    }

    // Errors are dismissed from any screen.
    if app.error.is_some() && key.code == KeyCode::Char('x') {
        app.error = None;
        return;
    }

    // Screens that own a text input must receive every key, including
    // the digits and letters the global layer would otherwise claim.
    let captured = app.ui.capture_input;

    // Ctrl+K (palette) and Ctrl+Q (quit) work even while a screen
    // input is active: they are modifier-guarded and never insert
    // characters into the screen's buffer.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('k') => {
                app.palette.open();
                return;
            }
            KeyCode::Char('q') => {
                app.quit = true;
                return;
            }
            _ => {}
        }
    }

    if !captured {
        match key.code {
            KeyCode::Tab => {
                app.next_screen();
                return;
            }
            KeyCode::BackTab => {
                app.prev_screen();
                return;
            }
            KeyCode::Esc => {
                app.back();
                return;
            }
            KeyCode::F(1) => {
                app.goto_fresh(ScreenId::Help);
                return;
            }
            KeyCode::F(12) => {
                app.quit = true;
                return;
            }
            KeyCode::Char('?') => {
                app.goto_fresh(ScreenId::Help);
                return;
            }
            KeyCode::Char('b') => {
                app.back();
                return;
            }
            KeyCode::Char('r') => {
                app.refresh_active_view();
                return;
            }
            KeyCode::Char(c @ '1'..='9') => {
                // Rail digit navigation to the primary sections.
                let index = c as usize - '1' as usize;
                if let Some((id, _)) = shell::PRIMARY_SECTIONS.get(index) {
                    app.goto_fresh(*id);
                }
                return;
            }
            _ => {}
        }
    }

    screens::handle_key(app, key);
}

/// Renders the shell, the active screen, then any overlay (palette,
/// confirmation dialog) on top. Resize-safe: every panel guards zero
/// dimensions and the shell returns None below the minimum.
fn draw(frame: &mut ratatui::Frame, app: &mut App<impl WorkspaceBackend>) {
    let area = frame.area();
    let Some(content) = shell::render_shell(frame, app, area) else {
        return;
    };
    screens::draw(frame, app, content);

    if app.palette.open {
        super::palette::render(frame, content, app);
    }

    if let Some(action) = &app.confirm {
        let destructive = action.kind.destructive();
        let describe = action.kind.describe();
        super::components::render_dialog(
            frame,
            area,
            if destructive { "Confirm" } else { "Run" },
            &[ratatui::text::Line::from(describe)],
            "[y] yes   [n/Esc] no",
            if destructive {
                super::theme::Role::Warning
            } else {
                super::theme::Role::Focused
            },
        );
    }

    if let Some(error) = &app.error {
        let error = error.clone();
        super::components::render_dialog(
            frame,
            area,
            "Error",
            &[ratatui::text::Line::from(error)],
            "[x] dismiss",
            super::theme::Role::Error,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::backend::LocalBackend;

    fn test_app() -> App<LocalBackend> {
        // Leak a tempdir for the app's lifetime; tests are short-lived.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        App::new(LocalBackend::new(root))
    }

    #[test]
    fn navigation_cycles_the_screen_ring() {
        // The premium ring puts the 13 primary sections first (Prompt
        // 30 navigation order), so Tab lands on Agents after the
        // Dashboard; Projects is a secondary screen now.
        let mut app = test_app();
        assert_eq!(app.screen(), ScreenId::Dashboard);
        app.next_screen();
        assert_eq!(app.screen(), ScreenId::Agents);
        app.prev_screen();
        assert_eq!(app.screen(), ScreenId::Dashboard);
    }

    #[test]
    fn goto_pushes_and_back_pops() {
        let mut app = test_app();
        app.goto(ScreenId::Files);
        app.goto(ScreenId::Editor);
        assert_eq!(app.screen(), ScreenId::Editor);
        app.back();
        assert_eq!(app.screen(), ScreenId::Files);
        app.back();
        assert_eq!(app.screen(), ScreenId::Dashboard);
    }

    #[test]
    fn escape_from_root_quits() {
        let mut app = test_app();
        app.back();
        assert!(app.quit());
    }

    #[test]
    fn destructive_actions_require_confirmation() {
        let mut app = test_app();
        app.request_action(ActionKind::DeleteProject("alpha".into()));
        assert!(app.confirm.is_some());
        app.confirm_pending();
        assert!(app.confirm.is_none());
    }

    #[test]
    fn confirmed_deletion_actually_deletes() {
        let mut app = test_app();
        app.backend.create_project("alpha").unwrap();
        app.request_action(ActionKind::DeleteProject("alpha".into()));
        app.confirm_pending();
        assert!(app.backend.list_projects().unwrap().is_empty());
    }

    #[test]
    fn cancelled_deletion_keeps_the_project() {
        let mut app = test_app();
        app.backend.create_project("alpha").unwrap();
        app.request_action(ActionKind::DeleteProject("alpha".into()));
        app.cancel_pending();
        assert_eq!(app.backend.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn discard_changes_confirms_and_reloads_buffer() {
        let mut app = test_app();
        app.backend.write_file("draft.txt", "original").unwrap();
        app.ui.editor_ui.path = Some("draft.txt".into());
        app.ui.editor_ui.buffer = "edited".into();
        app.ui.editor_ui.saved = Some("original".into());
        app.ui.editor_ui.dirty = true;

        // Discarding unsaved work is destructive: it confirms first.
        app.request_action(ActionKind::DiscardChanges("draft.txt".into()));
        assert!(app.confirm.is_some());
        app.confirm_pending();
        // The reload stash is populated for the Editor screen.
        let (path, content) = app.ui.reload_content.clone().unwrap();
        assert_eq!(path, "draft.txt");
        assert_eq!(content, "original");
        // The Editor adopts the fresh buffer on its next key event.
        app.goto(crate::tui::screens::ScreenId::Editor);
        crate::tui::screens::handle_key(
            &mut app,
            crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Null),
        );
        assert!(!app.ui.editor_ui.dirty);
        assert_eq!(app.ui.editor_ui.buffer, "original");
    }

    #[test]
    fn global_keys_are_handled_before_screens() {
        let mut app = test_app();
        handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.screen(), ScreenId::Agents);
        handle_key(&mut app, KeyEvent::from(KeyCode::F(1)));
        assert_eq!(app.screen(), ScreenId::Help);
    }

    #[test]
    fn digit_keys_jump_to_primary_sections() {
        // Digits index PRIMARY_SECTIONS (Dashboard takes 1):
        // 2 Agents, 3 Tasks, 4 Files, 5 Changes, 6 Git, 7 Context,
        // 8 Memory, 9 Skills.
        let mut app = test_app();
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('4')));
        assert_eq!(app.screen(), ScreenId::Files);
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('5')));
        assert_eq!(app.screen(), ScreenId::Changes);
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('6')));
        assert_eq!(app.screen(), ScreenId::Git);
    }

    #[test]
    fn ctrl_k_opens_the_command_palette() {
        let mut app = test_app();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        );
        assert!(app.palette.open);
        // Typing filters; Enter runs the first match (Go to Dashboard).
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('d')));
        handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
        assert!(!app.palette.open);
    }

    #[test]
    fn palette_refuses_disabled_commands() {
        let mut app = test_app();
        app.palette.open();
        // Empty workspace: project switch is disabled with a reason.
        let matches = super::super::palette::matches(&app);
        let disabled = matches
            .iter()
            .find(|c| !c.enabled)
            .expect("disabled command present");
        assert!(!disabled.disabled_reason.is_empty());
    }

    #[test]
    fn editor_input_capture_blocks_global_digits() {
        let mut app = test_app();
        app.goto(ScreenId::Editor);
        // Seed an open buffer (the editor ignores typing when no file
        // is open — 'o' is the only live key in that state).
        app.ui.editor_ui.path = Some("scratch.txt".into());
        app.ui.editor_ui.buffer = "seed".into();
        // The Editor screen always owns the keyboard once a key has
        // been dispatched to it (its handler sets capture_input).
        screens::handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
        assert!(app.ui.capture_input);
        // A digit must type into the buffer, not navigate.
        let before = app.ui.editor_ui.buffer.len();
        screens::handle_key(&mut app, KeyEvent::from(KeyCode::Char('1')));
        assert_eq!(app.ui.editor_ui.buffer.len(), before + 1);
        assert_eq!(app.screen(), ScreenId::Editor);
    }

    #[test]
    fn modal_confirmation_intercepts_all_keys() {
        let mut app = test_app();
        app.request_action(ActionKind::DeleteProject("alpha".into()));
        // Tab inside a modal must NOT navigate.
        handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.screen(), ScreenId::Dashboard);
        assert!(app.confirm.is_some());
        // 'y' confirms, ending the modal.
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('y')));
        assert!(app.confirm.is_none());
    }

    #[test]
    fn ctrl_q_quits_from_any_screen() {
        let mut app = test_app();
        let mut key = KeyEvent::from(KeyCode::Char('q'));
        key.modifiers = KeyModifiers::CONTROL;
        handle_key(&mut app, key);
        assert!(app.quit());
    }

    #[test]
    fn dashboard_cache_serves_within_ttl_and_refreshes_after_expiry() {
        let mut app = test_app();
        let first = app.dashboard_cached();
        assert_eq!(first.project_count, 0);

        // Within the TTL the cached snapshot is served unchanged.
        app.backend.create_project("alpha").unwrap();
        let cached = app.dashboard_cached();
        assert_eq!(cached.project_count, 0);

        // Manual invalidation (the 'r' key path) forces a recompute.
        app.invalidate_dashboard();
        let fresh = app.dashboard_cached();
        assert_eq!(fresh.project_count, 1);
    }

    #[test]
    fn dashboard_refresh_key_invalidates_cache() {
        let mut app = test_app();
        app.dashboard_cached();
        app.backend.create_project("alpha").unwrap();
        // 'r' is a global refresh now: it invalidates every view cache
        // (dashboard included) instead of only the dashboard.
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(app.dashboard_cached().project_count, 1);
        assert_eq!(app.message.as_deref(), Some("refreshed"));
    }

    #[test]
    fn dashboard_invalidated_after_confirmed_deletion() {
        let mut app = test_app();
        app.backend.create_project("alpha").unwrap();
        app.dashboard_cached();
        app.request_action(ActionKind::DeleteProject("alpha".into()));
        app.confirm_pending();
        assert_eq!(app.dashboard_cached().project_count, 0);
    }

    #[test]
    fn open_project_on_backend_tracks_current_context() {
        let mut app = test_app();
        app.backend.create_project("alpha").unwrap();
        app.backend.open_project("alpha").unwrap();
        assert_eq!(
            app.dashboard_cached().current_project.as_deref(),
            Some("alpha")
        );
    }
}
