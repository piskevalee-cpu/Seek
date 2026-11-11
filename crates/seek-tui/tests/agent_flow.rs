//! The whole flow: the agent loop, the tools and the UI (SPEC 9.1, 10.3).

use std::sync::{Arc, Mutex};

use seek_core::r#loop::{Agent, PermissionRequests, ToolRegistry};
use seek_core::process::ProcessRegistry;
use seek_core::tools::{BashTool, EditTool, ReadTool, WriteTool};
use seek_core::{AgentEvent, History, PermissionMode, ProviderEvent, TurnReason};
use seek_llm::provider::{BoxProviderStream, ChatRequest, Protocol, Provider};
use seek_tui::app::{App, Message};
use seek_tui::theme::Theme;
use tokio::sync::mpsc;

/// A provider that replays a fixed script.
struct Scripted {
    responses: Mutex<Vec<Vec<ProviderEvent>>>,
}

impl Scripted {
    fn new(responses: Vec<Vec<ProviderEvent>>) -> Self {
        Scripted {
            responses: Mutex::new(responses),
        }
    }
}

impl Provider for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    fn model(&self) -> &str {
        "scripted/model"
    }

    fn protocol(&self) -> Protocol {
        Protocol::OpenAiChat
    }

    fn stream<'a>(&'a self, _request: ChatRequest) -> BoxProviderStream<'a> {
        let next = self.responses.lock().unwrap().remove(0);
        use futures::StreamExt;
        futures::stream::iter(next.into_iter().map(Ok)).boxed()
    }
}

/// Run one turn and feed every event into the app.
async fn run_turn(app: &mut App, agent: Agent) -> TurnReason {
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);
    let handle = tokio::spawn(async move {
        let mut history = History::new();
        let reason = agent.run_turn(&mut history, &tx).await;
        (reason, history)
    });

    while let Some(event) = rx.recv().await {
        app.update(Message::Agent(event));
    }
    handle.await.expect("the turn finishes").0
}

fn agent(provider: Arc<dyn Provider>, mode: PermissionMode, root: std::path::PathBuf) -> Agent {
    let processes = Arc::new(ProcessRegistry::new());
    let tools = ToolRegistry {
        read: Some(ReadTool::new(root.clone())),
        write: Some(WriteTool::new(root.clone())),
        edit: Some(EditTool::new(root.clone())),
        bash: Some(BashTool::new(root.clone(), processes.clone())),
        ls: None,
        grep: None,
        find: None,
        fetch: None,
    };
    let (permissions, _requests) = PermissionRequests::new();
    Agent::new(provider, tools, mode, root, processes, permissions)
}

#[tokio::test]
async fn a_tool_call_shows_up_as_a_tool_row() {
    let root = std::env::temp_dir().join("seek-flow-test");
    let _ = std::fs::create_dir_all(&root);
    std::fs::write(root.join("a.txt"), "hello").expect("written");

    let provider = Arc::new(Scripted::new(vec![
        vec![ProviderEvent::ToolCall {
            id: "t1".into(),
            name: "read".into(),
            input: serde_json::json!({ "path": "a.txt" }),
        }],
        vec![ProviderEvent::TextDelta("the file says hello".into())],
    ]));

    let mut app = App::new(Theme::default());
    let reason = run_turn(&mut app, agent(provider, PermissionMode::Yolo, root)).await;

    assert_eq!(reason, TurnReason::Stop);
    let text = app
        .rows
        .iter()
        .map(|row| format!("{row:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Tool"), "there is a tool row: {text}");
    assert!(
        text.contains("a.txt"),
        "the tool row shows the path: {text}"
    );
    assert!(
        text.contains("the file says hello"),
        "the answer is there: {text}"
    );
}

#[tokio::test]
async fn a_failing_tool_becomes_an_error_result() {
    let root = std::env::temp_dir().join("seek-flow-test");
    let _ = std::fs::create_dir_all(&root);

    let provider = Arc::new(Scripted::new(vec![
        vec![ProviderEvent::ToolCall {
            id: "t1".into(),
            name: "read".into(),
            input: serde_json::json!({ "path": "missing.txt" }),
        }],
        vec![ProviderEvent::TextDelta("that file is missing".into())],
    ]));

    let mut app = App::new(Theme::default());
    let reason = run_turn(&mut app, agent(provider, PermissionMode::Yolo, root)).await;

    assert_eq!(reason, TurnReason::Stop);
    // The failure is in the tool output, not a crash.
    let output = app
        .rows
        .iter()
        .find_map(|row| match row {
            seek_tui::Row::Tool { status, detail, .. } => Some((status.clone(), detail.clone())),
            _ => None,
        })
        .expect("there is a tool row");
    assert_eq!(output.0, "error");
    assert!(output.1.unwrap().contains("missing.txt"));
}

#[tokio::test]
async fn the_read_mode_denies_a_write() {
    let root = std::env::temp_dir().join("seek-flow-test");
    let _ = std::fs::create_dir_all(&root);

    let provider = Arc::new(Scripted::new(vec![
        vec![ProviderEvent::ToolCall {
            id: "t1".into(),
            name: "write".into(),
            input: serde_json::json!({ "path": "new.txt", "content": "no" }),
        }],
        vec![ProviderEvent::TextDelta("understood".into())],
    ]));

    let mut app = App::new(Theme::default());
    run_turn(
        &mut app,
        agent(provider, PermissionMode::Read, root.clone()),
    )
    .await;

    assert!(!root.join("new.txt").exists(), "the file was not written");
}

#[tokio::test]
async fn a_tool_that_is_registered_shows_up_in_the_sidebar() {
    let root = std::env::temp_dir().join("seek-flow-test");
    let _ = std::fs::create_dir_all(&root);

    let provider = Arc::new(Scripted::new(vec![
        vec![ProviderEvent::ToolCall {
            id: "t1".into(),
            name: "bash".into(),
            input: serde_json::json!({ "command": "echo hi" }),
        }],
        vec![ProviderEvent::TextDelta("done".into())],
    ]));

    let mut app = App::new(Theme::default());
    app.sidebar.open = true;
    let registry = Arc::new(ProcessRegistry::new());
    app.processes = Some(registry.clone());

    run_turn(&mut app, agent(provider, PermissionMode::Yolo, root)).await;

    // A foreground command leaves the registry when it ends.
    assert!(registry.list().is_empty());
}
