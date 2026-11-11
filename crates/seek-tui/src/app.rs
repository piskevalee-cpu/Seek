//! The application (SPEC 4.2, 10).
//!
//! The UI follows The Elm Architecture: Model, Message, Update, View. `update` is pure — it never
//! does I/O — and side effects are commands that the binary sends to the agent.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use seek_core::{
    AgentEvent, History, Message as CoreMessage, PermissionDecision, ProcessRegistry,
    ProvidersConfig, Role, TurnReason,
};
use seek_llm::catalog::{ModelInfo, bundled_models};
use seek_local::model::{LocalModel, sidecar_path};
use seek_local::options::LaunchOptions;

use crate::footer::{Footer, ModelChip, PermissionMode, footer_line};
use crate::input::Input;
use crate::logo;
use crate::metrics::Metrics;
use crate::picker::{Picker, PickerItem};
use crate::prompt_bar;
use crate::sidebar::{self, Sidebar};
use crate::spinner::Spinner;
use crate::theme::Theme;
use crate::transcript::{Row, transcript_lines};
use crate::vitals::VitalsSampler;

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
    /// Hand a message to the agent, with the permission mode the user chose.
    Submit { text: String, mode: PermissionMode },
    /// Answer a permission request.
    Permission {
        id: u64,
        decision: PermissionDecision,
    },
    /// Switch to another model. The switch happens between turns (SPEC 7.3).
    SwitchModel { id: String },
    /// Summarize the older turns into one message (SPEC 8).
    Compact { messages: Vec<CoreMessage> },
    /// Write the toggles to the state.
    SetToggles(seek_core::Toggles),
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

/// What the prompt bar is doing (SPEC 9.4).
///
/// The bar has three modes: normal, the permission selector and the "tell SEEK what to do"
/// input.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptMode {
    /// The user is typing a message.
    Normal,
    /// The agent waits for a permission decision.
    Permission {
        id: u64,
        tool: String,
        summary: String,
        /// The selected choice: 0 yes, 1 no, 2 tell.
        choice: usize,
    },
    /// The user is telling SEEK what to do instead.
    Tell {
        id: u64,
        tool: String,
        summary: String,
    },
}

/// The choices of the permission selector.
pub const PERMISSION_CHOICES: [&str; 3] = ["Yes", "No", "Tell SEEK what to do"];

/// A centered overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    Help,
    /// A filterable list.
    Picker(Picker),
    /// The launch options of a local model (SPEC 7.4).
    Options {
        /// The id of the model.
        id: String,
        /// The name of the model.
        name: String,
        /// The read-only prefix of the command.
        prefix: String,
        /// The options of the user.
        input: Input,
    },
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
    /// The configured providers.
    pub providers: ProvidersConfig,
    /// The models that ship with the binary.
    pub catalog: Vec<ModelInfo>,
    /// The model that is currently selected.
    pub current: Option<ModelInfo>,
    /// The reasoning level of the current model.
    pub reasoning: Option<String>,
    /// What the prompt bar is doing.
    pub prompt_mode: PromptMode,
    /// The processes the agent started.
    pub processes: Option<std::sync::Arc<ProcessRegistry>>,
    /// The selected process in the sidebar.
    pub process_selected: usize,
    /// The local models of `~/seek/models`.
    pub local_models: Vec<LocalModel>,
    /// The log of the local server, shown in the log strip.
    pub logs: Option<std::sync::Arc<std::sync::Mutex<seek_local::LogBuffer>>>,
    /// The metrics of the current session.
    pub metrics: Metrics,
    /// The sampler of the vitals.
    pub vitals: VitalsSampler,
    /// The installed skills.
    pub skills: Vec<seek_core::Skill>,
    /// The external tools of `~/seek/tools`.
    pub external_tools: Vec<seek_core::ExternalTool>,
    /// Which tools and skills are enabled.
    pub toggles: seek_core::Toggles,
    /// When the turn started.
    turn_started: Option<std::time::Instant>,
    /// When the first reasoning token arrived.
    think_started: Option<std::time::Instant>,
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
            providers: ProvidersConfig::default(),
            catalog: bundled_models(),
            current: None,
            reasoning: None,
            prompt_mode: PromptMode::Normal,
            processes: None,
            process_selected: 0,
            local_models: Vec::new(),
            logs: None,
            metrics: Metrics::default(),
            vitals: VitalsSampler::new(),
            skills: Vec::new(),
            external_tools: Vec::new(),
            toggles: seek_core::Toggles::default(),
            turn_started: None,
            think_started: None,
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
                // The vitals are sampled once a second (SPEC 8).
                if self.vitals.is_due() {
                    self.metrics.vitals = Some(self.vitals.sample());
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

        // An open overlay takes the keyboard.
        match &self.overlay {
            Some(Overlay::Picker(_)) => return self.on_picker_key(key),
            Some(Overlay::Options { .. }) => return self.on_options_key(key),
            _ => {}
        }

        // The permission selector takes the keyboard too.
        if !matches!(self.prompt_mode, PromptMode::Normal) {
            return self.on_permission_key(key);
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
                if self.input.is_empty() {
                    self.move_selection(-1);
                } else {
                    self.input.history_prev();
                }
                None
            }
            KeyCode::Down => {
                if self.input.is_empty() {
                    self.move_selection(1);
                } else {
                    self.input.history_next();
                }
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
            KeyCode::Char('x') => {
                self.kill_selected_process();
                None
            }
            KeyCode::Char('t') => {
                if !self.cycle_reasoning() {
                    self.rows.push(Row::Error(
                        "reasoning is not configurable for this model".to_string(),
                    ));
                }
                None
            }
            _ => None,
        }
    }

    /// Handle a key while the prompt bar asks for a permission.
    fn on_permission_key(&mut self, key: KeyEvent) -> Option<Command> {
        match self.prompt_mode.clone() {
            PromptMode::Permission {
                id,
                tool,
                summary,
                choice,
            } => match key.code {
                KeyCode::Up => {
                    let next = (choice + PERMISSION_CHOICES.len() - 1) % PERMISSION_CHOICES.len();
                    self.prompt_mode = PromptMode::Permission {
                        id,
                        tool,
                        summary,
                        choice: next,
                    };
                    None
                }
                KeyCode::Down => {
                    let next = (choice + 1) % PERMISSION_CHOICES.len();
                    self.prompt_mode = PromptMode::Permission {
                        id,
                        tool,
                        summary,
                        choice: next,
                    };
                    None
                }
                KeyCode::Esc => {
                    // Esc denies the call.
                    self.prompt_mode = PromptMode::Normal;
                    Some(Command::Permission {
                        id,
                        decision: PermissionDecision::Deny,
                    })
                }
                KeyCode::Enter => match choice {
                    0 => {
                        self.prompt_mode = PromptMode::Normal;
                        Some(Command::Permission {
                            id,
                            decision: PermissionDecision::Allow,
                        })
                    }
                    1 => {
                        self.prompt_mode = PromptMode::Normal;
                        Some(Command::Permission {
                            id,
                            decision: PermissionDecision::Deny,
                        })
                    }
                    _ => {
                        // The third choice opens the text input in the same bar.
                        self.prompt_mode = PromptMode::Tell { id, tool, summary };
                        self.input = Input::new();
                        None
                    }
                },
                _ => None,
            },
            PromptMode::Tell { id, .. } => match key.code {
                KeyCode::Esc => {
                    self.prompt_mode = PromptMode::Normal;
                    Some(Command::Permission {
                        id,
                        decision: PermissionDecision::Deny,
                    })
                }
                KeyCode::Enter => {
                    let text = self.input.text().trim().to_string();
                    self.prompt_mode = PromptMode::Normal;
                    self.input = Input::new();
                    Some(Command::Permission {
                        id,
                        decision: PermissionDecision::Tell(text),
                    })
                }
                KeyCode::Backspace => {
                    self.input.backspace();
                    None
                }
                KeyCode::Char(c) => {
                    self.input.insert_char(c);
                    None
                }
                _ => None,
            },
            PromptMode::Normal => None,
        }
    }

    /// Handle a key while the launch options are edited.
    fn on_options_key(&mut self, key: KeyEvent) -> Option<Command> {
        let Some(Overlay::Options { id, input, .. }) = self.overlay.as_mut() else {
            return None;
        };
        match key.code {
            KeyCode::Esc => {
                self.overlay = None;
            }
            KeyCode::Enter => {
                // The text is taken out first, so the model can be written to.
                let text = input.text();
                let id = id.clone();
                self.overlay = None;
                self.save_options(&id, &text);
            }
            KeyCode::Backspace => {
                input.backspace();
            }
            KeyCode::Char(c) => {
                input.insert_char(c);
            }
            _ => {}
        }
        None
    }

    /// Open the launch options of a local model.
    fn open_options_editor(&mut self, id: &str) {
        let Some(local) = self.local_models.iter().find(|model| model.id == id) else {
            return;
        };
        let port = local.port().unwrap_or(0);
        let options = LaunchOptions::parse(local.sidecar.options.as_deref().unwrap_or_default());
        let mut input = Input::new();
        input.set_text(local.sidecar.options.as_deref().unwrap_or_default());
        let _ = options;
        self.overlay = Some(Overlay::Options {
            id: local.id.clone(),
            name: local.name.clone(),
            prefix: format!(
                "llama serve -m {} --host 127.0.0.1 --port {port}",
                local.path.display()
            ),
            input,
        });
    }

    /// Save the launch options of a local model.
    fn save_options(&mut self, id: &str, text: &str) {
        let Some(local) = self.local_models.iter_mut().find(|model| model.id == id) else {
            return;
        };
        let mut sidecar = local.sidecar.clone();
        sidecar.options = if text.trim().is_empty() {
            None
        } else {
            Some(text.trim().to_string())
        };
        if sidecar.write(&sidecar_path(&local.path)).is_ok() {
            local.sidecar = sidecar;
        }
    }

    /// Handle a key while a picker is open.
    fn on_picker_key(&mut self, key: KeyEvent) -> Option<Command> {
        let Some(Overlay::Picker(picker)) = self.overlay.as_mut() else {
            return None;
        };
        match key.code {
            KeyCode::Esc => {
                return self.close_picker(false);
            }
            KeyCode::Enter => {
                return self.close_picker(true);
            }
            KeyCode::Up => picker.move_prev(),
            KeyCode::Down => picker.move_next(),
            KeyCode::Right => {
                // The right arrow opens the launch options of a local model (SPEC 7.4).
                let selected = picker.selected_item().map(|item| item.value.clone());
                if let Some(value) = selected
                    && let Some(local_id) = value.strip_prefix("local:")
                {
                    self.open_options_editor(local_id);
                }
            }
            KeyCode::Backspace => picker.backspace(),
            KeyCode::Char(' ') => {
                // Space toggles the selected item of a checklist.
                let index = picker.selected;
                let visible = picker.visible_indices();
                if let Some(position) = visible.get(index)
                    && let Some(item) = picker.items.get_mut(*position)
                {
                    item.toggle();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                picker.type_char(c);
            }
            _ => {}
        }
        None
    }

    /// Close the picker and take the state of the checklist over into the toggles.
    fn close_picker(&mut self, choose: bool) -> Option<Command> {
        let Some(Overlay::Picker(picker)) = self.overlay.take() else {
            return None;
        };

        match picker.title.as_str() {
            "skills" => {
                for item in &picker.items {
                    if let Some(checked) = item.checked {
                        self.toggles.set_skill(&item.value, checked);
                    }
                }
            }
            "tools" => {
                for item in &picker.items {
                    if let Some(checked) = item.checked {
                        self.toggles.set_tool(&item.value, checked);
                    }
                }
            }
            _ => {}
        }

        let command = if choose {
            let chosen = picker.selected_item().map(|item| item.value.clone());
            match chosen {
                Some(value) => self.apply_picker_value(&value),
                None => None,
            }
        } else {
            None
        };

        // The toggles are written by the binary.
        Some(Command::SetToggles(self.toggles.clone())).or(command)
    }

    /// Apply what the user chose in a picker.
    fn apply_picker_value(&mut self, value: &str) -> Option<Command> {
        if let Some(model_id) = value.strip_prefix("model:") {
            if !self.switch_fits(model_id) {
                return None;
            }
            self.select_model(model_id);
            return Some(Command::SwitchModel {
                id: model_id.to_string(),
            });
        }
        if let Some(local_id) = value.strip_prefix("local:") {
            if !self.switch_fits(local_id) {
                return None;
            }
            self.select_local_model(local_id);
            return Some(Command::SwitchModel {
                id: local_id.to_string(),
            });
        }
        if value == "provider:add" {
            // Adding a provider needs a form; that lands with the provider editor.
            self.rows.push(Row::Error(
                "adding a provider needs the provider editor".to_string(),
            ));
            return None;
        }
        if let Some(model) = self
            .catalog
            .iter()
            .find(|model| format!("{}/{}", model.protocol.as_str(), model.id) == value)
        {
            let id = model.id.clone();
            self.select_model(&id);
            return Some(Command::SwitchModel { id });
        }
        None
    }

    /// Select a model from the catalog.
    pub fn select_model(&mut self, id: &str) {
        let Some(model) = self.catalog.iter().find(|model| model.id == id).cloned() else {
            return;
        };
        self.current = Some(model.clone());
        self.model = ModelChip {
            provider: model.protocol.as_str().to_string(),
            model: model.name.clone(),
            local: model.local,
        };
        // Switching to a model restores its level, else its default (SPEC 9.7).
        self.reasoning = model
            .resolve(self.reasoning.as_deref())
            .map(|variant| variant.id.clone());
        // The window of the model comes from the catalog (SPEC 8).
        self.metrics.context.total = model.context;
    }

    /// Select a local model.
    pub fn select_local_model(&mut self, id: &str) {
        let Some(local) = self
            .local_models
            .iter()
            .find(|model| model.id == id)
            .cloned()
        else {
            return;
        };
        self.current = None;
        self.model = ModelChip {
            provider: "local".to_string(),
            model: local.name.clone(),
            local: true,
        };
        self.reasoning = None;
    }

    /// Whether the conversation fits into the window of a model.
    ///
    /// A switch that would overflow is blocked: the user can compact or truncate instead
    /// (SPEC 8).
    fn switch_fits(&mut self, model_id: &str) -> bool {
        let window = self
            .catalog
            .iter()
            .find(|model| model.id == model_id)
            .map(|model| model.context)
            .unwrap_or(0);
        if window == 0 {
            return true;
        }
        let needed = seek_core::context::estimate_history(&self.history());
        if needed <= window as usize {
            return true;
        }
        self.rows.push(Row::Error(format!(
            "the conversation needs about {needed} tokens, the new model has {window}: \
             run /compact or /truncate first"
        )));
        false
    }

    /// Cycle the reasoning level of the current model (SPEC 9.7).
    pub fn cycle_reasoning(&mut self) -> bool {
        let Some(model) = &self.current else {
            return false;
        };
        if !model.has_variants() {
            return false;
        }
        let index = model
            .variants
            .iter()
            .position(|variant| Some(&variant.id) == self.reasoning.as_ref())
            .map(|index| (index + 1) % model.variants.len())
            .unwrap_or(0);
        self.reasoning = Some(model.variants[index].id.clone());
        true
    }

    /// Set the reasoning level of the current model.
    pub fn set_reasoning(&mut self, level: &str) -> bool {
        let Some(model) = &self.current else {
            return false;
        };
        if model.variant(level).is_none() {
            return false;
        }
        self.reasoning = Some(level.to_string());
        true
    }

    /// The picker for `/model`: the models of the configured providers, then the bundled catalog.
    fn model_picker(&self) -> Picker {
        let mut items: Vec<PickerItem> = Vec::new();
        for (id, provider) in &self.providers.provider {
            for (model_id, model) in &provider.models {
                if model.disabled.unwrap_or(false) {
                    continue;
                }
                let name = model.name.clone().unwrap_or_else(|| model_id.clone());
                let detail = match model.context {
                    Some(context) => format!("{context} context"),
                    None => String::new(),
                };
                items.push(
                    PickerItem::new(name, format!("model:{model_id}"))
                        .with_group(provider.display_name(id))
                        .with_detail(detail),
                );
            }
        }
        for local in &self.local_models {
            items.push(
                PickerItem::new(local.name.clone(), format!("local:{}", local.id))
                    .with_group("local")
                    .with_tag("LOCAL")
                    .with_detail(local.path.display().to_string()),
            );
        }
        if items.is_empty() {
            for model in &self.catalog {
                items.push(
                    PickerItem::new(model.name.clone(), format!("model:{}", model.id))
                        .with_group("catalog")
                        .with_detail(format!("{} context", model.context)),
                );
            }
        }
        Picker::new("models", items).tap_hint("Enter to choose, Esc to close")
    }

    /// The picker for `/skills`: a checklist of the installed skills.
    fn skills_picker(&self) -> Picker {
        let items: Vec<PickerItem> = self
            .skills
            .iter()
            .map(|skill| {
                PickerItem::new(skill.name.clone(), skill.name.clone())
                    .with_check(self.toggles.skill_enabled(&skill.name))
                    .with_detail(skill.description.clone())
            })
            .collect();
        Picker::new("skills", items).tap_hint("Space toggles, Enter closes, Esc closes")
    }

    /// The picker for `/tools`: a checklist of the built-in and the external tools.
    fn tools_picker(&self) -> Picker {
        let mut items: Vec<PickerItem> = Vec::new();
        for tool in seek_core::DEFAULT_TOOLS
            .iter()
            .chain(seek_core::HELPER_TOOLS.iter())
        {
            items.push(
                PickerItem::new(tool.name.clone(), tool.name.clone())
                    .with_check(self.toggles.tool_enabled(&tool.name))
                    .with_group("built-in")
                    .with_detail(tool.description.clone()),
            );
        }
        for tool in &self.external_tools {
            items.push(
                PickerItem::new(tool.config.name.clone(), tool.config.name.clone())
                    .with_check(self.toggles.tool_enabled(&tool.config.name))
                    .with_group("external")
                    .with_detail(tool.config.description.clone()),
            );
        }
        Picker::new("tools", items).tap_hint("Space toggles, Enter closes, Esc closes")
    }

    /// The picker for `/provider`: the configured providers.
    fn provider_picker(&self) -> Picker {
        let mut items: Vec<PickerItem> = Vec::new();
        for (id, provider) in &self.providers.provider {
            let kind = provider.kind.clone().unwrap_or_default();
            let detail = match (&provider.base_url, provider.kind.is_some()) {
                (Some(url), true) => format!("{kind}  {url}"),
                (Some(url), false) => url.clone(),
                (None, true) => kind,
                (None, false) => String::new(),
            };
            items.push(PickerItem::new(provider.display_name(id), id.clone()).with_detail(detail));
        }
        items.push(
            PickerItem::new("Add a provider", "provider:add")
                .with_detail("kind, base URL, key environment variable, models"),
        );
        Picker::new("providers", items).tap_hint("Enter to choose, Esc to close")
    }

    /// The lines of the log strip.
    ///
    /// The strip shows the last three lines of the local server and hides after ten seconds of
    /// silence (SPEC 10.3).
    fn log_strip_lines(&self) -> Vec<Line<'static>> {
        let Some(logs) = &self.logs else {
            return Vec::new();
        };
        let Ok(logs) = logs.lock() else {
            return Vec::new();
        };
        if logs.is_quiet(std::time::Duration::from_secs(10)) {
            return Vec::new();
        }
        logs.last(3)
            .into_iter()
            .map(|line| {
                Line::from(Span::styled(
                    line.text.clone(),
                    Style::new().fg(if line.error {
                        self.theme.warn
                    } else {
                        self.theme.dim
                    }),
                ))
            })
            .collect()
    }

    /// Copy the running processes into the sidebar.
    fn sync_processes(&mut self) {
        let Some(registry) = &self.processes else {
            return;
        };
        self.sidebar.processes = registry
            .list()
            .into_iter()
            .map(|entry| {
                let uptime = entry.uptime_text();
                (entry.pid, entry.tag, entry.command, uptime)
            })
            .collect();
        if self.process_selected >= self.sidebar.processes.len() {
            self.process_selected = self.sidebar.processes.len().saturating_sub(1);
        }
    }

    /// Kill the selected process of the sidebar.
    fn kill_selected_process(&mut self) {
        let Some(registry) = &self.processes else {
            return;
        };
        let processes = registry.list();
        if let Some(entry) = processes.get(self.process_selected) {
            registry.kill(entry.pid);
        }
    }

    /// Move the transcript selection up or down.
    ///
    /// Only tool rows can be selected, because only they can be expanded.
    fn move_selection(&mut self, delta: isize) {
        let selectable: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, Row::Tool { .. }))
            .map(|(index, _)| index)
            .collect();
        if selectable.is_empty() {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .and_then(|index| selectable.iter().position(|candidate| *candidate == index));
        let next = match current {
            None => {
                if delta < 0 {
                    selectable.len() - 1
                } else {
                    0
                }
            }
            Some(position) => {
                let length = selectable.len() as isize;
                let moved = (position as isize + delta).rem_euclid(length);
                moved as usize
            }
        };
        self.selected = Some(selectable[next]);
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

        // A skill is run with `/<skill-name> <prompt>`; its body goes into the turn (SPEC 9.5).
        let text = match self.match_skill(&text) {
            Some((skill, prompt)) => {
                if prompt.trim().is_empty() {
                    format!("{}\n", skill.body)
                } else {
                    format!("{}\n\n{prompt}", skill.body)
                }
            }
            None => text,
        };

        self.rows.push(Row::User(text.clone()));
        self.scroll = 0;
        self.selected = None;

        if self.status != Status::Idle {
            // The message is queued until the current turn ends.
            self.queued.push(text.clone());
            self.rows.push(Row::Pending(text));
            return None;
        }
        Some(Command::Submit {
            text,
            mode: self.mode,
        })
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
            "/compact" => {
                let messages = self.history();
                if messages.is_empty() {
                    return None;
                }
                Some(Command::Compact { messages })
            }
            "/truncate" => {
                self.truncate_to_fit();
                None
            }
            "/skills" => {
                self.overlay = Some(Overlay::Picker(self.skills_picker()));
                None
            }
            "/tools" => {
                self.overlay = Some(Overlay::Picker(self.tools_picker()));
                None
            }
            "/model" => {
                self.overlay = Some(Overlay::Picker(self.model_picker()));
                None
            }
            "/provider" => {
                self.overlay = Some(Overlay::Picker(self.provider_picker()));
                None
            }
            _ => None,
        }
    }

    /// The skill a text runs, and the prompt that goes with it.
    fn match_skill(&self, text: &str) -> Option<(seek_core::Skill, String)> {
        let (name, prompt) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        let name = name.strip_prefix('/')?;
        let skill = self
            .skills
            .iter()
            .find(|skill| skill.name == name)
            .cloned()?;
        if !self.toggles.skill_enabled(&skill.name) {
            return None;
        }
        Some((skill, prompt.trim().to_string()))
    }

    /// The slash command a text is, if any.
    fn slash_command(&self, text: &str) -> Option<&'static str> {
        match text {
            "/quit" => Some("/quit"),
            "/help" => Some("/help"),
            "/thoughts" => Some("/thoughts"),
            "/model" => Some("/model"),
            "/provider" => Some("/provider"),
            "/compact" => Some("/compact"),
            "/skills" => Some("/skills"),
            "/tools" => Some("/tools"),
            "/truncate" => Some("/truncate"),
            _ => None,
        }
    }

    /// Handle an event from the agent.
    fn on_agent_event(&mut self, event: AgentEvent) -> Option<Command> {
        match event {
            AgentEvent::TurnStarted => {
                self.status = Status::Thinking;
                self.spinner.tick();
                self.turn_started = Some(std::time::Instant::now());
                self.think_started = None;
                self.metrics.stats = seek_core::context::TurnStats::default();
            }
            AgentEvent::ThinkingDelta(text) => {
                if self.think_started.is_none() {
                    self.think_started = Some(std::time::Instant::now());
                }
                self.metrics.stats.had_reasoning = true;
                self.append_reasoning(&text);
            }
            AgentEvent::TextDelta(text) => {
                self.close_think_time();
                self.append_assistant(&text);
            }
            AgentEvent::ToolCallStarted { id, name } => {
                self.close_think_time();
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
                // The usage of the last response is the ground truth of the context (SPEC 8).
                self.metrics.context = self.metrics.context.with_usage(usage.input, usage.output);
                self.metrics.stats.output_tokens = usage.output;
            }
            AgentEvent::TurnFinished(reason) => {
                self.status = Status::Idle;
                self.close_think_time();
                if let Some(started) = self.turn_started.take() {
                    self.metrics.stats.generation_time = started.elapsed();
                }
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
                    return Some(Command::Submit {
                        text: joined,
                        mode: self.mode,
                    });
                }
            }
            AgentEvent::Compacted(summary) => {
                // The older turns become one message (SPEC 8).
                self.rows.clear();
                self.rows.push(Row::User("[compacted]".into()));
                self.rows.push(Row::Assistant(summary));
                self.scroll = 0;
            }
            AgentEvent::Error(message) => {
                self.rows.push(Row::Error(message));
                self.status = Status::Idle;
            }
            AgentEvent::PermissionRequest(request) => {
                self.prompt_mode = PromptMode::Permission {
                    id: request.id,
                    tool: request.tool,
                    summary: request.summary,
                    choice: 0,
                };
            }
        }
        None
    }

    /// Drop the oldest turns until the history fits into the window.
    ///
    /// Used when the history does not fit into the window of a new model (SPEC 8).
    pub fn truncate_to_fit(&mut self) -> bool {
        let window = self.metrics.context.total;
        if window == 0 {
            return false;
        }
        let mut history = History::new();
        for row in &self.rows {
            if let Some(message) = row_to_message(row) {
                history.push(message);
            }
        }
        let keep = seek_core::context::turns_that_fit(&history, window);
        if keep == history.messages.len() {
            return false;
        }
        // The oldest turns go; the newest ones stay.
        let drop = history.messages.len() - keep;
        let mut dropped = 0;
        self.rows.retain(|row| {
            let is_message = row_to_message(row).is_some();
            if is_message && dropped < drop {
                dropped += 1;
                return false;
            }
            true
        });
        true
    }

    /// Close the think time, when the model answered.
    fn close_think_time(&mut self) {
        if let Some(started) = self.think_started.take() {
            self.metrics.stats.think_time = started.elapsed();
        }
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
    pub fn view(&mut self, frame: &mut Frame) {
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
            self.sync_processes();
            sidebar::render(
                frame,
                sidebar_area,
                &self.sidebar,
                &self.theme,
                &self.metrics,
            );
        }

        let suggestions = prompt_bar::suggestion_lines(&self.input, &self.theme);
        let mut prompt_height = self.input.len().max(1) as u16;
        if self.status != Status::Idle {
            prompt_height += 1;
        }
        if !matches!(self.prompt_mode, PromptMode::Normal) {
            // The permission selector needs five lines: the question, the summary and the choices.
            prompt_height = prompt_height.max(5);
        }

        let log_lines = self.log_strip_lines();

        // The metrics have exactly one home: the bottom of the sidebar when it is open, the
        // line under the prompt bar otherwise. Never both (SPEC 10.2).
        let metrics_height: u16 = if self.sidebar.open && sidebar_area != Rect::ZERO {
            0
        } else {
            1
        };

        let [
            transcript_area,
            log_area,
            suggestions_area,
            prompt_area,
            metrics_area,
            footer_area,
        ] = Layout::vertical([
            Constraint::Min(3),
            Constraint::Length(log_lines.len() as u16),
            Constraint::Length(suggestions.len() as u16),
            Constraint::Length(prompt_height),
            Constraint::Length(metrics_height),
            Constraint::Length(1),
        ])
        .areas(main_area);

        if !log_lines.is_empty() {
            frame.render_widget(ratatui::widgets::Paragraph::new(log_lines), log_area);
        }

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
            &self.prompt_mode,
        );

        if metrics_height > 0 {
            crate::metrics::render_one_line(frame, metrics_area, &self.metrics, &self.theme);
        }

        let footer = Footer {
            mode: self.mode,
            model: self.model.clone(),
            reasoning: self.reasoning.clone(),
            cwd: self.cwd.clone(),
            branch: self.branch.clone(),
        };
        frame.render_widget(
            ratatui::widgets::Paragraph::new(footer_line(&footer, &self.theme)),
            footer_area,
        );

        match &self.overlay {
            Some(Overlay::Help) => self.render_help(frame, area),
            Some(Overlay::Picker(picker)) => picker.render(frame, area, &self.theme),
            Some(Overlay::Options {
                name,
                prefix,
                input,
                ..
            }) => self.render_options(frame, area, name, prefix, input),
            None => {}
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
                lines.push((
                    usize::MAX,
                    Line::from(Span::styled(
                        crate::spinner::STATIC_THINKING,
                        Style::new().fg(self.theme.dim),
                    )),
                ));
            } else {
                lines.extend(
                    self.spinner
                        .thinking_lines(&self.theme)
                        .into_iter()
                        .map(|line| (usize::MAX, line)),
                );
            }
        }

        // The selected row is highlighted, so it is clear what Ctrl+O will expand.
        let selected = self.selected;
        let lines: Vec<Line> = lines
            .into_iter()
            .map(|(index, line)| {
                if selected == Some(index) {
                    line.style(Style::new().bg(self.theme.cube_dark))
                } else {
                    line
                }
            })
            .collect();

        let height = area.height as usize;
        let total = lines.len();
        let end = total.min(height.saturating_add(self.scroll).min(total));
        let start = total.saturating_sub(height + self.scroll);
        let visible: Vec<Line> = lines[start.min(total)..end].to_vec();

        frame.render_widget(ratatui::widgets::Paragraph::new(visible), area);
    }

    /// The launch options editor.
    fn render_options(
        &self,
        frame: &mut Frame,
        area: Rect,
        name: &str,
        prefix: &str,
        input: &Input,
    ) {
        let width = area.width.min(76);
        let height = 7.min(area.height);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(ratatui::widgets::Clear, rect);
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::new().fg(self.theme.accent))
            .style(Style::new().bg(self.theme.bg))
            .title(Span::styled(
                format!(" launch options: {name} "),
                Style::new().fg(self.theme.accent).bold(),
            ));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        let lines = vec![
            Line::from(Span::styled(prefix, Style::new().fg(self.theme.dim))),
            Line::from(Span::styled(
                "the options above are set by SEEK",
                Style::new().fg(self.theme.dim).italic(),
            )),
            Line::from(vec![
                Span::styled("> ", Style::new().fg(self.theme.accent).bold()),
                Span::styled(input.text(), Style::new().fg(self.theme.cube_light)),
            ]),
            Line::from(Span::styled(
                "Enter saves, Esc returns",
                Style::new().fg(self.theme.dim).italic(),
            )),
        ];
        frame.render_widget(
            ratatui::widgets::Paragraph::new(lines),
            Rect::new(inner.x, inner.y, inner.width, inner.height),
        );
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
        frame.render_widget(ratatui::widgets::Clear, rect);
        let block = ratatui::widgets::Block::bordered()
            .border_style(Style::new().fg(self.theme.accent))
            .style(Style::new().bg(self.theme.bg))
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

/// The message of a transcript row, if it is one.
fn row_to_message(row: &Row) -> Option<CoreMessage> {
    match row {
        Row::User(text) => Some(CoreMessage::user(text)),
        Row::Assistant(text) => Some(CoreMessage::assistant(text)),
        _ => None,
    }
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
        assert_eq!(
            command,
            Some(Command::Submit {
                text: "hi".into(),
                mode: PermissionMode::Ask,
            })
        );
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
        assert_eq!(
            command,
            Some(Command::Submit {
                text: "hi".into(),
                mode: PermissionMode::Ask,
            })
        );
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
    fn up_and_down_select_a_tool_row() {
        let mut app = App::new(Theme::default());
        app.rows.push(Row::Tool {
            name: "bash".into(),
            command: "cargo test".into(),
            status: "ok".into(),
            duration: None,
            detail: None,
            expanded: false,
        });
        assert_eq!(app.selected, None);
        app.update(Message::Key(key(KeyCode::Down)));
        assert_eq!(app.selected, Some(0));
    }

    #[test]
    fn ctrl_o_expands_the_selected_tool_row() {
        let mut app = App::new(Theme::default());
        app.rows.push(Row::Tool {
            name: "bash".into(),
            command: "cargo test".into(),
            status: "ok".into(),
            duration: None,
            detail: Some("test result: ok".into()),
            expanded: false,
        });
        app.update(Message::Key(key(KeyCode::Down)));
        app.update(Message::Key(control('o')));
        match &app.rows[0] {
            Row::Tool { expanded, .. } => assert!(*expanded),
            other => panic!("expected a tool row, got {other:?}"),
        }
        app.update(Message::Key(control('o')));
        match &app.rows[0] {
            Row::Tool { expanded, .. } => assert!(!*expanded),
            other => panic!("expected a tool row, got {other:?}"),
        }
    }

    #[test]
    fn the_history_is_walked_when_there_is_input() {
        let mut app = App::new(Theme::default());
        app.input.insert_char('1');
        app.input.submit();
        app.input.insert_char('x');
        app.update(Message::Key(key(KeyCode::Up)));
        assert_eq!(app.input.text(), "1");
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
