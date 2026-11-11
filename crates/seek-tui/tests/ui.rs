//! UI tests with the ratatui `TestBackend` (SPEC 14).
//!
//! These tests render the real application into a buffer and check what ends up on the screen:
//! the Home screen, the Chat layout, the sidebar, the permission ask and the ownership table.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use seek_core::AgentEvent;
use seek_tui::app::{App, Message, Overlay};
use seek_tui::theme::Theme;
use seek_tui::transcript::Row;

/// A key press.
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Render the app into a buffer and return the screen as text.
fn render(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("the test backend works");
    terminal
        .draw(|frame| app.view(frame))
        .expect("drawing works");
    let buffer = terminal.backend().buffer();
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<Vec<_>>()
        .join("")
}

/// Render with line breaks, so that assertions can look at whole lines.
fn render_lines(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("the test backend works");
    terminal
        .draw(|frame| app.view(frame))
        .expect("drawing works");
    let buffer = terminal.backend().buffer();
    let area = buffer.area();
    let mut lines = Vec::new();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buffer.content[(y * area.width + x) as usize].symbol());
        }
        lines.push(line);
    }
    lines
}

fn app() -> App {
    let mut app = App::new(Theme::default());
    app.cwd = "/tmp".into();
    app.branch = Some("develop".into());
    app
}

#[test]
fn the_home_screen_shows_the_logo() {
    let mut app = app();
    let screen = render(&mut app, 100, 30);
    // The full-block logo is 50 columns wide and 9 rows tall.
    assert!(screen.contains("██"), "the logo is not on the home screen");
}

#[test]
fn a_small_terminal_says_so() {
    let mut app = app();
    let screen = render(&mut app, 40, 10);
    assert!(screen.contains("terminal too small"));
}

#[test]
fn the_chat_layout_shows_the_message_the_footer_and_the_prompt() {
    let mut app = app();
    app.rows.push(Row::User("hello".into()));
    app.rows.push(Row::Assistant("hi there".into()));
    let lines = render_lines(&mut app, 100, 30);

    assert!(lines.iter().any(|line| line.contains("hello")));
    assert!(lines.iter().any(|line| line.contains("hi there")));
    // The footer chip and the prompt marker.
    assert!(lines.iter().any(|line| line.contains("ASK")));
    assert!(lines.iter().any(|line| line.contains(">")));
}

#[test]
fn the_sidebar_can_be_shown() {
    let mut app = app();
    app.sidebar.open = true;
    let screen = render(&mut app, 120, 30);
    assert!(screen.contains("Sessions"));
    assert!(screen.contains("Processes"));
}

#[test]
fn the_sidebar_is_hidden_on_a_narrow_terminal() {
    let mut app = app();
    app.sidebar.open = true;
    let screen = render(&mut app, 90, 30);
    assert!(!screen.contains("Sessions"));
}

#[test]
fn the_mode_chip_is_the_only_place_the_mode_is_shown() {
    let mut app = app();
    app.mode = seek_tui::PermissionMode::Yolo;
    let screen = render(&mut app, 100, 30);
    assert_eq!(screen.matches("YOLO").count(), 1);
}

#[test]
fn the_model_is_shown_in_the_footer() {
    let mut app = app();
    app.model = seek_tui::ModelChip {
        provider: "anthropic".into(),
        model: "claude-sonnet".into(),
        local: false,
    };
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("claude-sonnet"));
    assert!(screen.contains("anthropic"));
}

#[test]
fn the_local_tag_only_shows_for_local_models() {
    let mut app = app();
    app.model.local = true;
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("LOCAL"));
}

#[test]
fn reasoning_is_shown_and_hidden() {
    let mut app = app();
    app.rows.push(Row::Reasoning("secret thinking".into()));
    let shown = render(&mut app, 100, 30);
    assert!(shown.contains("secret thinking"));

    app.show_reasoning = false;
    let hidden = render(&mut app, 100, 30);
    assert!(!hidden.contains("secret thinking"));
    assert!(hidden.contains("reasoning"));
}

#[test]
fn a_queued_message_is_tagged() {
    let mut app = app();
    app.rows.push(Row::Pending("later".into()));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("queued"));
}

#[test]
fn a_tool_row_shows_the_command_and_the_status() {
    let mut app = app();
    app.rows.push(Row::Tool {
        name: "bash".into(),
        command: "cargo test".into(),
        status: "ok".into(),
        duration: Some(std::time::Duration::from_millis(1200)),
        detail: Some("all good".into()),
        expanded: false,
    });
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("cargo test"));
    assert!(screen.contains("ok"));
    assert!(!screen.contains("all good"));
}

#[test]
fn the_help_overlay_lists_the_keys() {
    let mut app = app();
    app.overlay = Some(Overlay::Help);
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("Ctrl+B"));
    assert!(screen.contains("Alt+T"));
}

#[test]
fn the_spinner_appears_while_the_agent_thinks() {
    let mut app = app();
    app.update(Message::Agent(AgentEvent::TurnStarted));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("██"));
}

#[test]
fn the_ownership_table_holds_for_the_regions_that_exist() {
    // Every fact has exactly one home (SPEC 10.2): the mode chip only in the footer, the model
    // only in the footer, the reasoning text only in the transcript.
    let mut app = app();
    app.rows.push(Row::Reasoning("private".into()));
    app.model = seek_tui::ModelChip {
        provider: "fake".into(),
        model: "fake/demo".into(),
        local: false,
    };

    let lines = render_lines(&mut app, 100, 30);
    let footer = lines.last().expect("there is a footer");

    assert!(
        footer.contains("ASK"),
        "the mode chip belongs in the footer"
    );
    assert!(
        footer.contains("fake/demo"),
        "the model belongs in the footer"
    );

    let transcript: String = lines[..lines.len() - 1].join("\n");
    assert!(
        transcript.contains("private"),
        "reasoning belongs in the transcript"
    );
    assert!(
        !transcript.contains("ASK"),
        "the mode must not be shown outside the footer"
    );
}

#[test]
fn the_model_picker_lists_the_catalog() {
    let mut app = app();
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("Claude Sonnet 4.5"));
    assert!(screen.contains("GPT-5.4"));
}

#[test]
fn choosing_a_model_shows_it_in_the_footer() {
    let mut app = app();
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));
    // Enter takes the first model.
    app.update(Message::Key(key(KeyCode::Enter)));

    let lines = render_lines(&mut app, 100, 30);
    let footer = lines.last().expect("there is a footer");
    assert!(
        footer.contains("Claude Sonnet 4.5"),
        "the model is in the footer"
    );
}

#[test]
fn the_provider_picker_is_empty_without_configuration() {
    let mut app = app();
    for c in "/provider".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("Add a provider"));
}

#[test]
fn the_picker_can_be_filtered_and_closed() {
    let mut app = app();
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    for c in "gpt".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("GPT-5.4"));
    assert!(!screen.contains("Claude Sonnet 4.5"));

    app.update(Message::Key(key(KeyCode::Esc)));
    let screen = render(&mut app, 100, 30);
    assert!(!screen.contains("GPT-5.4"));
}

#[test]
fn a_permission_request_shows_the_three_choices() {
    let mut app = app();
    app.update(Message::Agent(AgentEvent::PermissionRequest(
        seek_core::PermissionRequest {
            id: 1,
            tool: "bash".into(),
            summary: "bash cargo test".into(),
        },
    )));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("Yes"));
    assert!(screen.contains("No"));
    assert!(screen.contains("Tell SEEK what to do"));
    assert!(screen.contains("cargo test"));
}

#[test]
fn the_permission_selector_answers_the_agent() {
    let mut app = app();
    app.update(Message::Agent(AgentEvent::PermissionRequest(
        seek_core::PermissionRequest {
            id: 7,
            tool: "bash".into(),
            summary: "bash ls".into(),
        },
    )));

    // Down to "No", then Enter.
    app.update(Message::Key(key(KeyCode::Down)));
    let command = app.update(Message::Key(key(KeyCode::Enter)));
    match command {
        Some(seek_tui::Command::Permission { id, decision }) => {
            assert_eq!(id, 7);
            assert_eq!(decision, seek_core::PermissionDecision::Deny);
        }
        other => panic!("expected a permission answer, got {other:?}"),
    }
}

#[test]
fn telling_seek_what_to_do_opens_a_text_input() {
    let mut app = app();
    app.update(Message::Agent(AgentEvent::PermissionRequest(
        seek_core::PermissionRequest {
            id: 3,
            tool: "write".into(),
            summary: "write a.txt".into(),
        },
    )));

    // Down twice to "Tell SEEK what to do", then Enter.
    app.update(Message::Key(key(KeyCode::Down)));
    app.update(Message::Key(key(KeyCode::Down)));
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("What should SEEK do instead?"));

    for c in "use other.txt".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    let command = app.update(Message::Key(key(KeyCode::Enter)));
    match command {
        Some(seek_tui::Command::Permission { id, decision }) => {
            assert_eq!(id, 3);
            assert_eq!(
                decision,
                seek_core::PermissionDecision::Tell("use other.txt".into())
            );
        }
        other => panic!("expected a permission answer, got {other:?}"),
    }
}

/// A local model for the tests.
fn a_local_model() -> seek_local::LocalModel {
    let dir = std::env::temp_dir().join("seek-ui-local-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("created");
    std::fs::write(dir.join("m.gguf"), b"x").expect("written");
    let models = seek_local::model::scan(&dir);
    models.into_iter().next().expect("the model is found")
}

#[test]
fn the_model_picker_shows_local_models() {
    let mut app = app();
    app.local_models = vec![a_local_model()];
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("LOCAL"));
    assert!(screen.contains("m"));
}

#[test]
fn the_right_arrow_opens_the_launch_options() {
    let mut app = app();
    app.local_models = vec![a_local_model()];
    app.llama_subcommand = Some("serve".into());
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    // The local model is the only one, so it is selected.
    app.update(Message::Key(key(KeyCode::Right)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("launch options"));
    // The implicit prefix is read-only.
    assert!(screen.contains("llama serve -m"));
    assert!(screen.contains("--host 127.0.0.1"));
}

#[test]
fn the_first_run_asks_which_model_to_use() {
    let mut app = app();
    app.local_models = vec![a_local_model()];
    app.update(Message::Start);

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("models"), "the picker is open");
    assert!(screen.contains("LOCAL"), "the local model is in it");
}

#[test]
fn the_first_run_asks_nothing_when_a_model_is_configured() {
    let mut app = app();
    app.local_models = vec![a_local_model()];
    app.select_local_model("local/m");
    app.model_configured = true;
    app.update(Message::Start);

    let screen = render(&mut app, 100, 30);
    // The picker is closed; the footer still shows the LOCAL tag of the model.
    assert!(!screen.contains("models"), "the picker stays closed");
    assert!(screen.contains("LOCAL"), "the footer shows the tag");
}

#[test]
fn the_launch_options_are_saved_to_the_sidecar() {
    let mut app = app();
    let model = a_local_model();
    app.local_models = vec![model.clone()];
    for c in "/model".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));
    app.update(Message::Key(key(KeyCode::Right)));

    for c in "--ctx-size 4096".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let sidecar = seek_local::model::Sidecar::read(&seek_local::model::sidecar_path(&model.path))
        .expect("the sidecar is written");
    assert_eq!(sidecar.options.as_deref(), Some("--ctx-size 4096"));
}

#[test]
fn the_metrics_line_is_under_the_prompt_bar() {
    let mut app = app();
    app.metrics.context = seek_core::context::ContextWindow::new(32_000).with_usage(12_400, 0);
    let lines = render_lines(&mut app, 100, 30);
    let metrics = lines
        .iter()
        .find(|line| line.contains("12.4k / 32.0k"))
        .expect("the metrics are shown");
    assert!(metrics.contains("39%"));
}

#[test]
fn the_metrics_are_in_the_sidebar_when_it_is_open() {
    let mut app = app();
    app.sidebar.open = true;
    app.metrics.context = seek_core::context::ContextWindow::new(32_000).with_usage(12_400, 0);
    let lines = render_lines(&mut app, 120, 30);

    // The counts are there exactly once: in the sidebar, not under the prompt bar.
    let count = lines
        .iter()
        .filter(|line| line.contains("12.4k / 32.0k"))
        .count();
    assert_eq!(count, 1, "the metrics have exactly one home");
}

#[test]
fn the_vitals_are_shown() {
    let mut app = app();
    app.metrics.vitals = Some(seek_tui::Vitals {
        cpu: Some(12.0),
        ram_used: Some(1024),
        ram_total: Some(2048),
        ..seek_tui::Vitals::default()
    });
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("cpu 12%"));
}

#[test]
fn the_context_is_updated_by_the_usage_of_the_response() {
    let mut app = app();
    app.metrics.context = seek_core::context::ContextWindow::new(32_000);
    app.update(Message::Agent(AgentEvent::Usage(seek_core::Usage {
        input: 1000,
        output: 200,
    })));
    assert_eq!(app.metrics.context.used, 1200);
    assert!(!app.metrics.context.estimated);
}

#[test]
fn compacting_asks_the_model_for_a_summary() {
    let mut app = app();
    app.rows.push(Row::User("add a test".into()));
    app.rows.push(Row::Assistant("done".into()));

    for c in "/compact".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    let command = app.update(Message::Key(key(KeyCode::Enter)));
    match command {
        Some(seek_tui::Command::Compact { messages }) => {
            assert_eq!(messages.len(), 2);
            assert_eq!(messages[0].plain_text(), "add a test");
        }
        other => panic!("expected a compact command, got {other:?}"),
    }
}

#[test]
fn the_compacted_conversation_becomes_one_message() {
    let mut app = app();
    app.rows.push(Row::User("add a test".into()));
    app.rows.push(Row::Assistant("done".into()));

    app.update(Message::Agent(AgentEvent::Compacted(
        "the user wanted a test".into(),
    )));

    let text: String = app
        .rows
        .iter()
        .map(|row| format!("{row:?}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("the user wanted a test"));
    assert!(!text.contains("add a test"));
}

/// A skill for the tests.
fn a_skill(name: &str, description: &str) -> seek_core::Skill {
    seek_core::Skill {
        name: name.to_string(),
        description: description.to_string(),
        path: std::path::PathBuf::from("/skills")
            .join(name)
            .join("SKILL.md"),
        body: format!("the body of {name}"),
        allowed_tools: Vec::new(),
        license: None,
        compatibility: None,
        metadata: None,
    }
}

#[test]
fn the_skills_picker_is_a_checklist() {
    let mut app = app();
    app.skills = vec![a_skill("init", "Write an AGENTS.md.")];

    for c in "/skills".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("[x]"));
    assert!(screen.contains("init"));
    assert!(screen.contains("Write an AGENTS.md."));
}

#[test]
fn space_toggles_a_skill() {
    let mut app = app();
    app.skills = vec![a_skill("init", "Write an AGENTS.md.")];

    for c in "/skills".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));
    // Space toggles the selected skill off.
    app.update(Message::Key(key(KeyCode::Char(' '))));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("[ ]"));

    // Closing the picker takes the state over into the toggles.
    app.update(Message::Key(key(KeyCode::Enter)));
    assert!(!app.toggles.skill_enabled("init"));
}

#[test]
fn a_skill_is_run_with_its_body() {
    let mut app = app();
    app.skills = vec![a_skill("init", "Write an AGENTS.md.")];

    for c in "/init write it".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    let command = app.update(Message::Key(key(KeyCode::Enter)));
    match command {
        Some(seek_tui::Command::Submit { text, .. }) => {
            assert!(text.contains("the body of init"), "{text}");
            assert!(text.contains("write it"), "{text}");
        }
        other => panic!("expected a submit, got {other:?}"),
    }
}

#[test]
fn a_disabled_skill_is_not_run() {
    let mut app = app();
    app.skills = vec![a_skill("init", "Write an AGENTS.md.")];
    app.toggles.set_skill("init", false);

    for c in "/init".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    let command = app.update(Message::Key(key(KeyCode::Enter)));
    match command {
        Some(seek_tui::Command::Submit { text, .. }) => {
            assert_eq!(text, "/init", "the skill did not run");
        }
        other => panic!("expected a submit, got {other:?}"),
    }
}

#[test]
fn the_tools_picker_shows_the_built_in_tools() {
    let mut app = app();
    for c in "/tools".chars() {
        app.update(Message::Key(key(KeyCode::Char(c))));
    }
    app.update(Message::Key(key(KeyCode::Enter)));

    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("read"));
    assert!(screen.contains("bash"));
    assert!(screen.contains("[x]"));
}

#[test]
fn the_home_screen_shows_the_widgets() {
    let mut app = app();
    app.widget_outputs = vec![seek_core::WidgetOutput {
        name: "git".into(),
        title: "recent commits".into(),
        lines: vec!["on develop".into(), "abc1234 feat: something".into()],
        failed: false,
    }];
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("recent commits"));
    assert!(screen.contains("on develop"));
    assert!(screen.contains("abc1234"));
}

#[test]
fn a_failing_widget_shows_its_title_and_a_dash() {
    let mut app = app();
    app.widget_outputs = vec![seek_core::WidgetOutput {
        name: "git".into(),
        title: "recent commits".into(),
        lines: vec!["-".into()],
        failed: true,
    }];
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("recent commits"));
    assert!(screen.contains("-"));
}

#[test]
fn the_widgets_disappear_with_the_first_message() {
    let mut app = app();
    app.widget_outputs = vec![seek_core::WidgetOutput {
        name: "git".into(),
        title: "recent commits".into(),
        lines: vec!["on develop".into()],
        failed: false,
    }];
    app.rows.push(Row::User("hello".into()));
    let screen = render(&mut app, 100, 30);
    assert!(!screen.contains("recent commits"));
}

#[test]
fn the_rate_limit_only_shows_when_it_is_reported() {
    let mut app = app();
    app.update(Message::Agent(AgentEvent::RateLimit(
        seek_core::provider::RateLimit {
            limit: Some(50),
            remaining: Some(49),
            resets_in: None,
        },
    )));
    let screen = render(&mut app, 100, 30);
    assert!(screen.contains("49/50"));
}
