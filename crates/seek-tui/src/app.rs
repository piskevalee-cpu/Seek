//! The application (SPEC 4.2, 10).
//!
//! The UI follows The Elm Architecture: Model, Message, Update, View. `update` is pure — it never
//! does I/O — and side effects are commands that the binary sends to the agent.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use seek_core::{AgentEvent, Message as CoreMessage, Role, TurnReason};

use crate::footer::{Footer, ModelChip, PermissionMode, footer_line};
use crate::input::Input;
use crate::logo;
use crate::prompt_bar;
use crate::sidebar::{self, Sidebar};
use crate::spinner::Spinner;
use crate::theme::Theme;
use crate::transcript::{Row, transcript_lines};

/// The smallest usable terminal.
pub const MIN_WIDTH: u16 = 80;
/// The smallest usable terminal.
pub const MIN_HEIGHT: u16 = 24;

/// Below this width the sidebar hides itself.
pub const SIDEBAR_MIN_WIDTH: u16 = 100;

/// A tick is sent every 100 ms (SPEC 4.2).
pub const TICK_MS: u64 = 100;

/// Everything that can happen to the application.
#[derive(Debug)]
pub enum Message {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    Agent(AgentEvent),
}

/// A side effect that the binary has to perform.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Hand a message to the agent.
    Submit(String),
    /// Leave.
    Quit,
}

/// What the agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Thinking,
    Loading,
}

/// A centered overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Help,
}

/// The application state.
#[derive(Debug)]
pub struct App {
    pub theme: Theme,
    pub mode: PermissionMode,
    pub model: ModelChip,
    pub rows: Vec<Row>,
    pub input: Input,
    pub sidebar: Sidebar,
    pub show_reasoning: bool,
    pub spinner: Spinner,
    pub status: Status,
    /// Messages typed while the agent is working.
    pub queued: Vec<String>,
    /// How far the transcript is scrolled up; 0 means stuck to the bottom.
    pub scroll: usize,
    /// The selected transcript row.
    pub selected: Option<usize>,
    pub overlay: Option<Overlay>,
    pub should_quit: bool,
    pub reduce_motion: bool,
    pub cwd: String,
    pub branch: Option<String>,
    /// Whether the last update changed something that needs a redraw.
    dirty: bool,
    /// The last usage reported by the provider.
    pub last_usage: Option<(u32, u32)>,
    /// How often Ctrl+C was pressed in a row.
    last_ctrl_c: u8,
}

impl App {
    pub fn new(theme: Theme) -> Self {
        App {
            theme,
            mode: PermissionMode::Ask,
            model: ModelChip {
                provider: "fake".into(),
                model: "fake/demo".into(),
                local: false,
            },
            rows: Vec::new(),
            input: Input::new(),
            sidebar: Sidebar::default(),
            show_reasoning: true,
            spinner: Spinner::new(),
            status: Status::Idle,
            queued: Vec::new(),
            scroll: 0,
            selected: None,
            overlay: None,
            should_quit: false,
            reduce_motion: false,
            cwd: String::new(),
            branch: None,
            dirty: true,
            last_usage: None,
            last_ctrl_c: 0,
        }
    }

    /// Whether the last update changed anything that has to be drawn.
    pub fn needs_redraw(&self) -> bool {
        self.dirty
    }

    /// Handle a message.
    pub fn update(&mut self, message: Message) -> Option<Command> {
        self.dirty = true;
        match message {
            Message::Key(key) => self.on_key(key),
            Message::Resize(_, _) => None,
            Message::Tick => {
                if self.status != Status::Idle {
                    self.spinner.tick();
                }
                None
            }
            Message::Agent(event) => self.on_agent_event(event),
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Option<Command> {
        // Release and repeat events are not presses.
        if key.kind != KeyEventKind::Press {
            return None;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return self.on_control_key(key);
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            if key.code == KeyCode::Char('t') {
                self.show_reasoning = !self.show_reasoning;
            }
            return None;
        }

        match key.code {
            KeyCode::Enter => {
                if key
                    .modifiers
                    .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
                {
                    self.input.insert_newline();
                    return None;
                }
                self.submit()
            }
            KeyCode::Esc => {
                if self.overlay.is_some() {
                    self.overlay = None;
                    return None;
                }
                // Interrupting a turn arrives with the next agent event.
                self.scroll = 0;
                None
            }
            KeyCode::Backspace => {
                self.input.backspace();
                None
            }
            KeyCode::Delete => {
                self.input.delete();
                None
            }
            KeyCode::Left => {
                self.input.move_left();
                None
            }
            KeyCode::Right => {
                self.input.move_right();
                None
            }
            KeyCode::Up => {
                self.input.history_prev();
                None
            }
            KeyCode::Down => {
                self.input.history_next();
                None
            }
            KeyCode::Home => {
                self.input.move_home();
                None
            }
            KeyCode::End => {
                self.input.move_end();
                None
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_add(1);
                None
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_sub(1);
                None
            }
            KeyCode::Tab => {
                self.mode = self.mode.next();
                None
            }
            KeyCode::Char(c) => {
                self.last_ctrl_c = 0;
                self.input.insert_char(c);
                None
            }
            _ => None,
        }
    }

    fn on_control_key(&mut self, key: KeyEvent) -> Option<Command> {
        match key.code {
            KeyCode::Char('c') => {
                // Ctrl+C clears the input; twice it quits.
                self.last_ctrl_c += 1;
                if !self.input.is_empty() {
                    self.input = Input::new();
                }
                if self.last_ctrl_c >= 2 {
                    self.should_quit = true;
                    return Some(Command::Quit);
                }
                None
            }
            KeyCode::Char('b') => {
                self.sidebar.toggle();
                None
            }
            KeyCode::Char('o') => {
                self.toggle_selected_row();
                None
            }
            _ => None,
        }
    }

    /// Expand or collapse the selected tool row.
    fn toggle_selected_row(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        if let Some(Row::Tool { expanded, .. }) = self.rows.get_mut(index) {
            *expanded = !*expanded;
        }
    }

    /// Send the current input.
    fn submit(&mut self) -> Option<Command> {
        let text = self.input.text().trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input = Input::new();

        if let Some(command) = self.slash_command(&text) {
            return self.run_slash_command(command);
        }

        self.rows.push(Row::User(text.clone()));
        self.scroll = 0;
        self.selected = None;

        if self.status != Status::Idle {
            // The message is queued until the current turn ends.
            self.queued.push(text.clone());
            self.rows.push(Row::Pending(text));
            return None;
        }
        Some(Command::Submit(text))
    }

    /// Run a slash command.
    fn run_slash_command(&mut self, command: &str) -> Option<Command> {
        match command {
            "/quit" => {
                self.should_quit = true;
                Some(Command::Quit)
            }
            "/help" => {
                self.overlay = Some(Overlay::Help);
                None
            }
            "/thoughts" => {
                self.show_reasoning = !self.show_reasoning;
                None
            }
            _ => None,
        }
    }

    /// The slash command a text is, if any.
    fn slash_command(&self, text: &str) -> Option<&'static str> {
        match text {
            "/quit" => Some("/quit"),
            "/help" => Some("/help"),
            "/thoughts" => Some("/thoughts"),
            _ => None,
        }
    }

    /// Handle an event from the agent.
    fn on_agent_event(&mut self, event: AgentEvent) -> Option<Command> {
        match event {
            AgentEvent::TurnStarted => {
                self.status = Status::Thinking;
                self.spinner.tick();
            }
            AgentEvent::ThinkingDelta(text) => {
                self.append_reasoning(&text);
            }
            AgentEvent::TextDelta(text) => {
                self.append_assistant(&text);
            }
            AgentEvent::ToolCallStarted { id, name } => {
                self.rows.push(Row::Tool {
                    name,
                    command: id,
                    status: "running".into(),
                    duration: None,
                    detail: None,
                    expanded: false,
                });
            }
            AgentEvent::ToolCallReady { name, input, .. } => {
                if let Some(Row::Tool { command, .. }) = self.rows.last_mut() {
                    *command = summarize_input(&input);
                }
                let _ = name;
            }
            AgentEvent::ToolOutput(output) => {
                if let Some(Row::Tool {
                    status,
                    duration,
                    detail,
                    ..
                }) = self.rows.last_mut()
                {
                    *status = if output.is_error {
                        "error".into()
                    } else {
                        "ok".into()
                    };
                    *duration = Some(output.duration);
                    *detail = Some(output.content);
                }
            }
            AgentEvent::Usage(usage) => {
                self.last_usage = Some((usage.input, usage.output));
            }
            AgentEvent::TurnFinished(reason) => {
                self.status = Status::Idle;
                if reason == TurnReason::Failed {
                    self.rows
                        .push(Row::Error("the turn ended with an error".into()));
                }
                // Queued messages are delivered as one user message when the turn finishes.
                if !self.queued.is_empty() {
                    let joined = self.queued.join("\n");
                    self.queued.clear();
                    self.rows.retain(|row| !matches!(row, Row::Pending(_)));
                    self.rows.push(Row::User(joined.clone()));
                    return Some(Command::Submit(joined));
                }
            }
            AgentEvent::Error(message) => {
                self.rows.push(Row::Error(message));
                self.status = Status::Idle;
            }
            AgentEvent::PermissionRequest(_) => {}
        }
        None
    }

    /// Append to the reasoning row, or start one.
    fn append_reasoning(&mut self, text: &str) {
        match self.rows.last_mut() {
            Some(Row::Reasoning(existing)) => existing.push_str(text),
            _ => self.rows.push(Row::Reasoning(text.to_string())),
        }
    }

    /// Append to the assistant row, or start one.
    fn append_assistant(&mut self, text: &str) {
        match self.rows.last_mut() {
            Some(Row::Assistant(existing)) => existing.push_str(text),
            _ => self.rows.push(Row::Assistant(text.to_string())),
        }
    }

    /// The conversation so far, in the canonical model.
    pub fn history(&self) -> Vec<CoreMessage> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                Row::User(text) => Some(CoreMessage::new(
                    Role::User,
                    vec![seek_core::Block::text(text)],
                )),
                Row::Assistant(text) => Some(CoreMessage::new(
                    Role::Assistant,
                    vec![seek_core::Block::text(text)],
                )),
                _ => None,
            })
            .collect()
    }

    /// Draw the application.
    pub fn view(&self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            frame.render_widget(ratatui::widgets::Paragraph::new("terminal too small"), area);
            return;
        }

        let (sidebar_area, main_area) = if self.sidebar.open && area.width >= SIDEBAR_MIN_WIDTH {
            let width = sidebar::SIDEBAR_WIDTH;
            (
                Rect::new(area.x, area.y, width, area.height),
                Rect::new(area.x + width, area.y, area.width - width, area.height),
            )
        } else {
            (Rect::ZERO, area)
        };
        if self.sidebar.open && sidebar_area != Rect::ZERO {
            sidebar::render(frame, sidebar_area, &self.sidebar, &self.theme);
        }

        let suggestions = prompt_bar::suggestion_lines(&self.input, &self.theme);
        let mut prompt_height = self.input.len().max(1) as u16;
        if self.status != Status::Idle {
            prompt_height += 1;
        }

        let [transcript_area, suggestions_area, prompt_area, footer_area] = Layout::vertical([
            Constraint::Min(3),
            Constraint::Length(suggestions.len() as u16),
            Constraint::Length(prompt_height),
            Constraint::Length(1),
        ])
        .areas(main_area);

        if !suggestions.is_empty() {
            frame.render_widget(
                ratatui::widgets::Paragraph::new(suggestions),
                suggestions_area,
            );
        }

        self.render_transcript(frame, transcript_area);

        prompt_bar::render(
            frame,
            prompt_area,
            &self.input,
            &self.theme,
            self.status != Status::Idle,
            self.queued.len(),
        );

        let footer = Footer {
            mode: self.mode,
            model: self.model.clone(),
            reasoning: None,
            cwd: self.cwd.clone(),
            branch: self.branch.clone(),
        };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(footer_line(&footer, &self.theme)),
            footer_area,
        );

        if let Some(Overlay::Help) = self.overlay {
            self.render_help(frame, area);
        }
    }

    /// Draw the transcript, or the home screen when there is nothing to show.
    fn render_transcript(&self, frame: &mut Frame, area: Rect) {
        if self.rows.is_empty() && self.status == Status::Idle {
            // Home: the logo, centered. Widgets land in milestone v0.7.
            logo::render(frame, area, &self.theme);
            return;
        }

        let mut lines = transcript_lines(&self.rows, &self.theme, self.show_reasoning);
        if self.status == Status::Thinking {
            if self.reduce_motion {
                lines.push(Line::from(Span::styled(
                    crate::spinner::STATIC_THINKING,
                    Style::new().fg(self.theme.dim),
                )));
            } else {
                lines.extend(self.spinner.thinking_lines(&self.theme));
            }
        }

        let height = area.height as usize;
        let total = lines.len();
        let end = total.min(height.saturating_add(self.scroll).min(total));
        let start = total.saturating_sub(height + self.scroll);
        let visible: Vec<Line> = lines[start.min(total)..end].to_vec();

        frame.render_widget(ratatui::widgets::Paragraph::new(visible), area);
    }

    /// The `/help` overlay.
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let width = area.width.min(60);
        let height = area.height.min(18);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::new().fg(self.theme.accent))
            .title(Span::styled(" Keys ", Style::new().fg(self.theme.accent)));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let keys = [
            ("Enter", "send"),
            ("Shift+Enter", "newline"),
            ("Esc", "interrupt / close"),
            ("Shift+Tab", "cycle permission mode"),
            ("Ctrl+B", "toggle sidebar"),
            ("Ctrl+O", "expand a tool row"),
            ("Ctrl+C", "clear input, twice to quit"),
            ("PgUp / PgDn", "scroll the transcript"),
            ("Alt+T", "show or hide reasoning"),
        ];
        let lines: Vec<Line> = keys
            .iter()
            .map(|(key, description)| {
                Line::from(vec![
                    Span::styled(format!("  {key:<12}"), Style::new().fg(self.theme.accent)),
                    Span::styled(*description, Style::new().fg(self.theme.cube_light)),
                ])
            })
            .collect();
        frame.render_widget(
            ratatui::widgets::Paragraph::new(lines),
            Rect::new(inner.x, inner.y, inner.width, inner.height),
        );
    }
}

/// A one-line summary of a tool call.
fn summarize_input(input: &serde_json::Value) -> String {
    let Some(object) = input.as_object() else {
        return input.to_string();
    };
    for key in ["command", "path", "pattern"] {
        if let Some(value) = object.get(key).and_then(|v| v.as_str()) {
            return value.to_string();
        }
    }
    object
        .iter()
        .map(|(key, value)| match value.as_str() {
            Some(text) => format!("{key}={text}"),
            None => key.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn control(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn typing_goes_into_the_input() {
        let mut app = App::new(Theme::default());
        for c in "hi".chars() {
            app.update(Message::Key(key(KeyCode::Char(c))));
        }
        assert_eq!(app.input.text(), "hi");
    }

    #[test]
    fn enter_submits_the_message() {
        let mut app = App::new(Theme::default());
        for c in "hi".chars() {
            app.update(Message::Key(key(KeyCode::Char(c))));
        }
        let command = app.update(Message::Key(key(KeyCode::Enter)));
        assert_eq!(command, Some(Command::Submit("hi".into())));
        assert_eq!(app.rows.len(), 1);
    }

    #[test]
    fn shift_enter_inserts_a_newline() {
        let mut app = App::new(Theme::default());
        app.update(Message::Key(key(KeyCode::Char('a'))));
        app.update(Message::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::SHIFT,
        )));
        app.update(Message::Key(key(KeyCode::Char('b'))));
        assert_eq!(app.input.text(), "a\nb");
    }

    #[test]
    fn ctrl_b_toggles_the_sidebar() {
        let mut app = App::new(Theme::default());
        app.update(Message::Key(control('b')));
        assert!(app.sidebar.open);
        app.update(Message::Key(control('b')));
        assert!(!app.sidebar.open);
    }

    #[test]
    fn ctrl_c_clears_then_quits() {
        let mut app = App::new(Theme::default());
        app.update(Message::Key(key(KeyCode::Char('x'))));
        app.update(Message::Key(control('c')));
        assert!(app.input.is_empty());
        assert_eq!(app.update(Message::Key(control('c'))), Some(Command::Quit));
    }

    #[test]
    fn alt_t_toggles_reasoning() {
        let mut app = App::new(Theme::default());
        assert!(app.show_reasoning);
        app.update(Message::Key(KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::ALT,
        )));
        assert!(!app.show_reasoning);
    }

    #[test]
    fn tab_cycles_the_permission_mode() {
        let mut app = App::new(Theme::default());
        assert_eq!(app.mode, PermissionMode::Ask);
        app.update(Message::Key(key(KeyCode::Tab)));
        assert_eq!(app.mode, PermissionMode::Yolo);
        app.update(Message::Key(key(KeyCode::Tab)));
        assert_eq!(app.mode, PermissionMode::Read);
    }

    #[test]
    fn a_message_typed_while_working_is_queued() {
        let mut app = App::new(Theme::default());
        app.update(Message::Agent(AgentEvent::TurnStarted));
        for c in "hi".chars() {
            app.update(Message::Key(key(KeyCode::Char(c))));
        }
        assert_eq!(app.update(Message::Key(key(KeyCode::Enter))), None);
        assert_eq!(app.queued, vec!["hi".to_string()]);
        assert!(app.rows.iter().any(|row| matches!(row, Row::Pending(_))));
    }

    #[test]
    fn queued_messages_are_delivered_when_the_turn_finishes() {
        let mut app = App::new(Theme::default());
        app.update(Message::Agent(AgentEvent::TurnStarted));
        for c in "hi".chars() {
            app.update(Message::Key(key(KeyCode::Char(c))));
        }
        app.update(Message::Key(key(KeyCode::Enter)));
        let command = app.update(Message::Agent(AgentEvent::TurnFinished(TurnReason::Stop)));
        assert_eq!(command, Some(Command::Submit("hi".into())));
        assert!(app.queued.is_empty());
        assert!(!app.rows.iter().any(|row| matches!(row, Row::Pending(_))));
    }

    #[test]
    fn reasoning_and_text_are_appended_to_their_rows() {
        let mut app = App::new(Theme::default());
        app.update(Message::Agent(AgentEvent::ThinkingDelta("th".into())));
        app.update(Message::Agent(AgentEvent::ThinkingDelta("ink".into())));
        app.update(Message::Agent(AgentEvent::TextDelta("an".into())));
        app.update(Message::Agent(AgentEvent::TextDelta("swer".into())));
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], Row::Reasoning("think".into()));
        assert_eq!(app.rows[1], Row::Assistant("answer".into()));
    }

    #[test]
    fn slash_commands_run_instead_of_being_sent() {
        let mut app = App::new(Theme::default());
        for c in "/help".chars() {
            app.update(Message::Key(key(KeyCode::Char(c))));
        }
        assert_eq!(app.update(Message::Key(key(KeyCode::Enter))), None);
        assert_eq!(app.overlay, Some(Overlay::Help));
        assert!(app.rows.is_empty());
    }

    #[test]
    fn the_thoughts_command_toggles_reasoning() {
        let mut app = App::new(Theme::default());
        for c in "/thoughts".chars() {
            app.update(Message::Key(key(KeyCode::Char(c))));
        }
        app.update(Message::Key(key(KeyCode::Enter)));
        assert!(!app.show_reasoning);
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut app = App::new(Theme::default());
        let mut event = key(KeyCode::Char('a'));
        event.kind = KeyEventKind::Release;
        app.update(Message::Key(event));
        assert!(app.input.is_empty());
    }
}
