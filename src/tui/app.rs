//! TUI application state and event loop.

use anyhow::Context;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::backend::WorkspaceBackend;
use super::screens::{self, ScreenId, ScreenState, SCREENS};

/// A concrete action the user asked the UI to perform. Destructive
/// actions wait for modal confirmation before `execute` runs them.
#[derive(Debug, Clone)]
pub enum ActionKind {
    DeleteProject(String),
    DeletePath(String),
    /// Discard unsaved editor changes for the given path.
    DiscardChanges(String),
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

    /// Requests an action. Destructive actions are held until the user
    /// confirms the modal; non-destructive ones execute immediately.
    pub fn request_action(&mut self, kind: ActionKind) {
        let action = PendingAction { kind };
        if action.kind.destructive() {
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
    /// the message/error bars. A discarded editor buffer is stashed in
    /// `reload_content` so the Editor screen can adopt it on its next
    /// draw.
    fn execute(&mut self, kind: ActionKind) {
        // DiscardChanges is UI-local: reload the editor buffer through
        // the backend and stash it for the Editor screen to adopt.
        if let ActionKind::DiscardChanges(path) = &kind {
            match self.backend.read_file(path) {
                Ok(fresh) => {
                    self.ui.reload_content = Some((path.clone(), fresh));
                    self.set_message(format!("done: {}", kind.describe()));
                }
                Err(e) => self.set_error(format!("{}: {e:#}", kind.describe())),
            }
            return;
        }
        let result = match &kind {
            ActionKind::DeleteProject(name) => self
                .backend
                .delete_project(name)
                .with_context(|| format!("delete project {name}")),
            ActionKind::DeletePath(path) => self
                .backend
                .delete_path(path)
                .with_context(|| format!("delete {path}")),
            ActionKind::DiscardChanges(_) => unreachable!("handled above"),
        };
        match result {
            Ok(()) => self.set_message(format!("done: {}", kind.describe())),
            Err(e) => self.set_error(format!("{}: {e:#}", kind.describe())),
        }
    }

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

fn handle_key<B: WorkspaceBackend>(app: &mut App<B>, key: KeyEvent) {
    // Global keys first; modal confirmation takes precedence over
    // everything so a destructive action can never be triggered while
    // another confirmation is pending.
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => app.confirm_pending(),
            KeyCode::Char('n') | KeyCode::Esc => app.cancel_pending(),
            _ => {}
        }
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char('q') = key.code {
            app.quit = true;
            return;
        }
    }
    match key.code {
        KeyCode::Tab => app.next_screen(),
        KeyCode::BackTab => app.prev_screen(),
        KeyCode::Esc => app.back(),
        KeyCode::F(1) => app.goto_fresh(ScreenId::Help),
        KeyCode::F(12) => app.quit = true,
        _ => screens::handle_key(app, key),
    }
}

fn draw(frame: &mut Frame, app: &mut App<impl WorkspaceBackend>) {
    let small = frame.area().width < 60 || frame.area().height < 12;
    let [body, footer] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(if small { 1 } else { 3 }),
    ])
    .areas(frame.area());

    if small {
        draw_small(frame, app, body);
    } else {
        screens::draw(frame, app, body);
    }
    draw_footer(frame, app, footer, small);
}

/// Ultra-condensed rendering for very small terminals: title plus a
/// hint that the full layout needs more space.
fn draw_small(
    frame: &mut Frame,
    app: &mut App<impl WorkspaceBackend>,
    area: ratatui::layout::Rect,
) {
    let screen = SCREENS
        .iter()
        .find(|s| s.id == app.screen())
        .map(|s| s.title)
        .unwrap_or("AWH");
    let line = Line::from(vec![
        Span::styled("AWH", Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled(screen, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  [Tab/±] screens  [Esc] back  [F1] help"),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(
    frame: &mut Frame,
    app: &mut App<impl WorkspaceBackend>,
    area: ratatui::layout::Rect,
    small: bool,
) {
    let block = if small {
        None
    } else {
        Some(Block::default().borders(Borders::TOP))
    };
    let mut lines: Vec<Line> = Vec::new();
    if let Some(action) = &app.confirm {
        let verb = if action.kind.destructive() {
            "CONFIRM"
        } else {
            "RUN"
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{verb}: "), Style::default().fg(Color::Yellow)),
            Span::styled(action.kind.describe(), Style::default().fg(Color::Red)),
            Span::raw("  [y] yes   [n/Esc] no"),
        ]));
    } else if let Some(error) = &app.error {
        lines.push(Line::from(vec![
            Span::styled("error: ", Style::default().fg(Color::Red)),
            Span::raw(error.clone()),
            Span::raw("  [x] dismiss"),
        ]));
    } else if let Some(message) = &app.message {
        lines.push(Line::from(vec![
            Span::styled("i: ", Style::default().fg(Color::Cyan)),
            Span::raw(message.clone()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back  "),
            Span::styled("F1", Style::default().fg(Color::Cyan)),
            Span::raw(" help  "),
            Span::styled("C-q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ]));
    }
    let mut paragraph = Paragraph::new(lines);
    if let Some(block) = block {
        paragraph = paragraph.block(block);
    }
    frame.render_widget(paragraph, area);
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
        let mut app = test_app();
        assert_eq!(app.screen(), ScreenId::Dashboard);
        app.next_screen();
        assert_eq!(app.screen(), ScreenId::Projects);
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
        assert_eq!(app.screen(), ScreenId::Projects);
        handle_key(&mut app, KeyEvent::from(KeyCode::F(1)));
        assert_eq!(app.screen(), ScreenId::Help);
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
}
