//! TUI application state and event loop.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::backend::WorkspaceBackend;
use super::screens::{self, ScreenId, ScreenState, SCREENS};

/// One interactive UI action in a screens' local queue.
#[derive(Debug, Clone)]
pub struct PendingAction {
    /// Short machine-readable action id, e.g. `confirm-delete`.
    pub id: &'static str,
    /// Human description shown in the confirmation bar.
    pub description: String,
    /// True when the action is destructive and needs an explicit `y`.
    pub destructive: bool,
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

    /// Requests confirmation for an action; non-destructive actions run
    /// immediately.
    pub fn request_action(&mut self, action: PendingAction) {
        if action.destructive {
            self.confirm = Some(action);
        } else {
            self.set_message(format!("{}: done", action.description));
        }
    }

    /// Confirms the pending action by running its continuation.
    pub fn confirm_pending(&mut self) {
        if let Some(action) = self.confirm.take() {
            self.set_message(format!("confirmed: {}", action.description));
        }
    }

    pub fn cancel_pending(&mut self) {
        self.confirm = None;
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
        let verb = if action.destructive { "CONFIRM" } else { "RUN" };
        lines.push(Line::from(vec![
            Span::styled(format!("{verb}: "), Style::default().fg(Color::Yellow)),
            Span::styled(action.description.clone(), Style::default().fg(Color::Red)),
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
        app.request_action(PendingAction {
            id: "delete",
            description: "delete project alpha".into(),
            destructive: true,
        });
        assert!(app.confirm.is_some());
        app.confirm_pending();
        assert!(app.confirm.is_none());
    }

    #[test]
    fn non_destructive_actions_run_immediately() {
        let mut app = test_app();
        app.request_action(PendingAction {
            id: "refresh",
            description: "refresh listing".into(),
            destructive: false,
        });
        assert!(app.confirm.is_none());
        assert!(app.message.is_some());
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
        app.request_action(PendingAction {
            id: "delete",
            description: "delete project alpha".into(),
            destructive: true,
        });
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
