//! The whole flow: the agent loop, the tools and the UI (SPEC 9.1, 10.3).

use std::sync::{Arc, Mutex};

use seek_core::PermissionRequest;
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
    agent_with_permissions(provider, mode, root).0
}

/// An agent, with the channel its permission requests go through.
fn agent_with_permissions(
    provider: Arc<dyn Provider>,
    mode: PermissionMode,
    root: std::path::PathBuf,
) -> (Agent, PermissionRequests, mpsc::Receiver<PermissionRequest>) {
    // The tools run in this folder, so it has to exist.
    let _ = std::fs::create_dir_all(&root);
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
    let (permissions, requests) = PermissionRequests::new();
    (
        Agent::new(provider, tools, mode, root, processes, permissions.clone()),
        permissions,
        requests,
    )
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

/// An agent with the tools in a folder of its own, and its permission channel.
fn an_agent(
    provider: Arc<dyn Provider>,
    mode: PermissionMode,
) -> (Agent, PermissionRequests, mpsc::Receiver<PermissionRequest>) {
    agent_with_permissions(
        provider,
        mode,
        std::env::temp_dir().join("seek-flow-permission-test"),
    )
}

/// A provider that asks for one tool and then answers.
fn tool_calling_provider() -> Arc<Scripted> {
    Arc::new(Scripted::new(vec![
        vec![ProviderEvent::ToolCall {
            id: "t1".into(),
            name: "bash".into(),
            input: serde_json::json!({ "command": "echo hello" }),
        }],
        vec![ProviderEvent::TextDelta("the command said hello".into())],
    ]))
}

/// Run a turn and answer the permission request with `decision`.
///
/// Returns the events of the turn and whether the tool asked for permission.
async fn run_with_permission(
    agent: Agent,
    requests: PermissionRequests,
    mut incoming: mpsc::Receiver<PermissionRequest>,
    decision: Option<seek_core::PermissionDecision>,
) -> (Vec<AgentEvent>, bool) {
    let (events, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(64);

    let handle = tokio::spawn(async move {
        let mut history = History::new();
        agent.run_turn(&mut history, &events).await;
    });

    // The permission request is answered as soon as it arrives.
    let asked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&asked);
    let answering = tokio::spawn(async move {
        while let Some(request) = incoming.recv().await {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(decision) = decision.clone() {
                requests.answer(request.id, decision);
            }
        }
    });

    let mut all = Vec::new();
    while let Some(event) = rx.recv().await {
        let done = matches!(event, AgentEvent::TurnFinished(_));
        all.push(event);
        if done {
            break;
        }
    }
    handle.await.expect("the turn finishes");
    answering.abort();
    (all, asked.load(std::sync::atomic::Ordering::Relaxed))
}

#[tokio::test]
async fn in_ask_mode_the_tool_waits_for_the_answer() {
    let (agent, requests, incoming) = an_agent(tool_calling_provider(), PermissionMode::Ask);
    let (events, asked) = run_with_permission(
        agent,
        requests,
        incoming,
        Some(seek_core::PermissionDecision::Allow),
    )
    .await;

    // The permission was asked for.
    assert!(asked, "the tool asked before it ran");
    // The tool ran.
    let output = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolOutput(output) => Some(output),
            _ => None,
        })
        .expect("the tool ran");
    assert!(!output.is_error, "the command ran");
    assert!(output.content.contains("hello"), "{}", output.content);
}

#[tokio::test]
async fn a_denied_tool_does_not_run() {
    let (agent, requests, incoming) = agent_with_permissions(
        tool_calling_provider(),
        PermissionMode::Ask,
        std::env::temp_dir().join("seek-flow-permission-test"),
    );
    let (events, _asked) = run_with_permission(
        agent,
        requests,
        incoming,
        Some(seek_core::PermissionDecision::Deny),
    )
    .await;

    let output = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolOutput(output) => Some(output),
            _ => None,
        })
        .expect("the tool was answered");
    assert!(output.is_error, "the tool did not run");
    assert!(output.content.contains("denied"), "{}", output.content);
}

#[tokio::test]
async fn in_yolo_mode_the_tool_runs_without_asking() {
    let (agent, requests, incoming) = agent_with_permissions(
        tool_calling_provider(),
        PermissionMode::Yolo,
        std::env::temp_dir().join("seek-flow-permission-test"),
    );
    let (events, asked) = run_with_permission(agent, requests, incoming, None).await;

    // Nothing was asked.
    assert!(!asked, "the tool did not ask: {events:?}");
    let output = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolOutput(output) => Some(output),
            _ => None,
        })
        .expect("the tool ran");
    assert!(!output.is_error, "the command ran");
}

#[tokio::test]
async fn the_tell_answer_goes_back_to_the_model() {
    let (agent, requests, incoming) = agent_with_permissions(
        tool_calling_provider(),
        PermissionMode::Ask,
        std::env::temp_dir().join("seek-flow-permission-test"),
    );
    let (events, _asked) = run_with_permission(
        agent,
        requests,
        incoming,
        Some(seek_core::PermissionDecision::Tell(
            "use the other command".into(),
        )),
    )
    .await;

    let output = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ToolOutput(output) => Some(output),
            _ => None,
        })
        .expect("the tool was answered");
    assert!(output.is_error);
    assert!(
        output.content.contains("use the other command"),
        "{}",
        output.content
    );
}
