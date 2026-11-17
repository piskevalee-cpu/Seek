//! The agent loop of the binary, with the commands of the user (SPEC 4.1, 9.1, 9.4).
//!
//! The loop used to wait for the turn to end before it took the next command: the answer to
//! a permission request and Esc never arrived, and the turn hung. These tests drive the
//! loop the way the interface does, against a small server that speaks the OpenAI wire
//! protocol.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use seek::Runtime;
use seek_core::r#loop::PermissionRequests;
use seek_core::{AgentEvent, PermissionDecision, PermissionMode, ProvidersConfig};
use seek_tui::app::Command;
use tokio::sync::mpsc;

/// How long a test waits before it gives up.
const PATIENCE: Duration = Duration::from_secs(10);

/// A runtime with a home of its own.
fn a_runtime(name: &str) -> Arc<Runtime> {
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("the home is created");
    Arc::new(Runtime::new(&home, "llama", Some("serve".into())))
}

/// A small server that speaks the OpenAI wire protocol.
///
/// The first answer asks for a tool; the second one is the answer of the model.
async fn a_model_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the server listens");
    let port = listener.local_addr().expect("the port is known").port();

    tokio::spawn(async move {
        let mut turn = 0;
        while let Ok((mut socket, _)) = listener.accept().await {
            // The whole request is read: the answer must not come while the client is
            // still sending, or the client sees a broken connection.
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                match tokio::io::AsyncReadExt::read(&mut socket, &mut byte).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => head.push(byte[0]),
                }
            }
            let text = String::from_utf8_lossy(&head).to_lowercase();
            let length = text
                .split("content-length:")
                .nth(1)
                .and_then(|rest| rest.split('\r').next())
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if length > 0 {
                let mut body = vec![0u8; length];
                let _ = tokio::io::AsyncReadExt::read_exact(&mut socket, &mut body).await;
            }

            let events = if turn == 0 {
                turn += 1;
                vec![
                    r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"bash","arguments":"{\"command\":\"echo hello\"}"}}]}}]}"#,
                ]
            } else {
                turn += 1;
                vec![r#"{"choices":[{"index":0,"delta":{"content":"the command said hello"}}]}"#]
            };

            let mut body = String::new();
            for event in events {
                body.push_str(&format!("data: {event}\n\n"));
            }
            body.push_str("data: [DONE]\n\n");

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
            let _ = tokio::io::AsyncWriteExt::shutdown(&mut socket).await;
        }
    });

    port
}

/// A server that accepts the request and never answers.
///
/// The turn waits for the model for ever, so only a cancel can end it.
async fn a_silent_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the server listens");
    let port = listener.local_addr().expect("the port is known").port();

    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            // The socket is kept open, and nothing is ever sent on it.
            held.push(socket);
        }
    });

    port
}

/// Run the agent loop of the binary, with the commands of the user.
fn the_loop(
    runtime: Arc<Runtime>,
    port: u16,
) -> (mpsc::Sender<Command>, mpsc::Receiver<AgentEvent>) {
    let (command_tx, command_rx) = mpsc::channel::<Command>(16);
    let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);
    let (widget_tx, _widget_rx) = mpsc::channel::<seek_tui::app::UiMessage>(16);
    let (permission_tx, mut permission_rx) = PermissionRequests::new();

    // The permission requests of the agent are forwarded to the interface, the way the
    // binary does it.
    let events = event_tx.clone();
    tokio::spawn(async move {
        while let Some(request) = permission_rx.recv().await {
            if events
                .send(AgentEvent::PermissionRequest(request))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // The provider is the small server above.
    let mut providers = ProvidersConfig::default();
    providers.provider.insert(
        "test".into(),
        seek_core::ProviderConfig {
            name: Some("test".into()),
            kind: Some("openai-chat".into()),
            base_url: Some(format!("http://127.0.0.1:{port}/v1")),
            env: vec!["SEEK_TEST_KEY".into()],
            models: Default::default(),
        },
    );

    tokio::spawn(seek::agent_loop(
        command_rx,
        event_tx,
        widget_tx,
        providers,
        permission_tx,
        runtime,
    ));

    (command_tx, event_rx)
}

/// Wait for an event that matches.
async fn wait_for<F>(events: &mut mpsc::Receiver<AgentEvent>, matches: F) -> Option<AgentEvent>
where
    F: Fn(&AgentEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + PATIENCE;
    while tokio::time::Instant::now() < deadline {
        let Ok(event) = tokio::time::timeout(PATIENCE, events.recv()).await else {
            continue;
        };
        let Some(event) = event else { break };
        if matches(&event) {
            return Some(event);
        }
    }
    None
}

#[tokio::test]
async fn the_answer_to_a_permission_request_arrives() {
    // The tool asks, the user says yes, and the tool runs. Before, the answer never
    // arrived: the loop was waiting for the turn to end, and the turn was waiting for the
    // answer.
    // The provider needs a key to be picked up (the key is never sent anywhere: the
    // server of the test does not want it).
    unsafe {
        std::env::set_var("SEEK_TEST_KEY", "test");
    }
    let runtime = a_runtime("seek-loop-permission");
    let port = a_model_server().await;
    let (commands, mut events) = the_loop(Arc::clone(&runtime), port);

    commands
        .send(Command::Submit {
            text: "run the command".into(),
            mode: PermissionMode::Ask,
        })
        .await
        .expect("the command is sent");

    // The permission is asked for.
    let request = wait_for(&mut events, |event| {
        matches!(event, AgentEvent::PermissionRequest(_))
    })
    .await
    .expect("the permission is asked for");
    let id = match request {
        AgentEvent::PermissionRequest(request) => request.id,
        _ => unreachable!(),
    };

    // The answer of the user goes back through the loop.
    commands
        .send(Command::Permission {
            id,
            decision: PermissionDecision::Allow,
        })
        .await
        .expect("the command is sent");

    // The tool runs, and the turn ends.
    let output = wait_for(&mut events, |event| {
        matches!(event, AgentEvent::ToolOutput(_))
    })
    .await
    .expect("the tool runs");
    match output {
        AgentEvent::ToolOutput(output) => {
            assert!(!output.is_error, "the command ran: {}", output.content);
            assert!(output.content.contains("hello"), "{}", output.content);
        }
        _ => unreachable!(),
    }
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnFinished(_))
        })
        .await
        .is_some(),
        "the turn ends"
    );
}

#[tokio::test]
async fn the_cancel_stops_the_turn_that_is_running() {
    // The model never answers: the turn would run for ever without the cancel.
    unsafe {
        std::env::set_var("SEEK_TEST_KEY", "test");
    }
    let runtime = a_runtime("seek-loop-cancel");
    let port = a_silent_server().await;
    let (commands, mut events) = the_loop(Arc::clone(&runtime), port);

    commands
        .send(Command::Submit {
            text: "hello".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");

    // Wait for the turn to start, so the cancel lands on a running turn.
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnStarted)
        })
        .await
        .is_some(),
        "the turn starts"
    );

    commands
        .send(Command::Cancel)
        .await
        .expect("the command is sent");

    // The turn ends. Without the cancel it would wait for the model for ever.
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnFinished(_))
        })
        .await
        .is_some(),
        "the turn ends after the cancel"
    );
}

#[tokio::test]
async fn a_message_after_the_cancel_is_taken() {
    // The bug the user found: Esc, then a message, and nothing happened at all. The
    // message has to be taken after the cancel.
    unsafe {
        std::env::set_var("SEEK_TEST_KEY", "test");
    }
    let runtime = a_runtime("seek-loop-after-cancel");
    let port = a_silent_server().await;
    let (commands, mut events) = the_loop(Arc::clone(&runtime), port);

    commands
        .send(Command::Submit {
            text: "the first message".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnStarted)
        })
        .await
        .is_some(),
        "the turn starts"
    );

    commands
        .send(Command::Cancel)
        .await
        .expect("the command is sent");
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnFinished(_))
        })
        .await
        .is_some(),
        "the turn ends"
    );

    // The next message is taken.
    commands
        .send(Command::Submit {
            text: "the second message".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnStarted)
        })
        .await
        .is_some(),
        "the second turn starts"
    );
}

#[tokio::test]
async fn the_cancel_stops_the_turn_that_is_running_and_not_the_next_one() {
    // Two turns: the first one runs, the second one waits for the conversation. The cancel
    // has to stop the first one. When the second turn took the token before it waited, the
    // cancel stopped the wrong turn and the first one ran for ever.
    let runtime = a_runtime("seek-loop-two-turns");
    let port = a_silent_server().await;
    let (commands, mut events) = the_loop(Arc::clone(&runtime), port);

    commands
        .send(Command::Submit {
            text: "the first message".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnStarted)
        })
        .await
        .is_some(),
        "the first turn starts"
    );

    // The second turn is submitted while the first one runs.
    commands
        .send(Command::Submit {
            text: "the second message".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");

    // The cancel stops the turn that is running.
    commands
        .send(Command::Cancel)
        .await
        .expect("the command is sent");

    // The first turn ends, and the second one takes over.
    let cancelled = wait_for(&mut events, |event| {
        matches!(
            event,
            AgentEvent::TurnFinished(seek_core::TurnReason::Cancelled)
        )
    })
    .await;
    assert!(cancelled.is_some(), "the running turn is cancelled");

    // The second turn starts: the conversation was free again.
    let started = wait_for(&mut events, |event| {
        matches!(event, AgentEvent::TurnStarted)
    })
    .await;
    assert!(started.is_some(), "the next turn takes over");

    // And it can be stopped as well.
    commands
        .send(Command::Cancel)
        .await
        .expect("the command is sent");
    assert!(
        wait_for(&mut events, |event| {
            matches!(
                event,
                AgentEvent::TurnFinished(seek_core::TurnReason::Cancelled)
            )
        })
        .await
        .is_some(),
        "the next turn is cancelled too"
    );
}

#[tokio::test]
async fn the_mode_of_the_message_is_the_mode_of_the_session() {
    // The message carries the mode. The turn must follow it: the tool runs without asking
    // when the message says so.
    let runtime = a_runtime("seek-loop-mode");
    let port = a_model_server().await;
    let (commands, mut events) = the_loop(Arc::clone(&runtime), port);

    commands
        .send(Command::Submit {
            text: "run the command".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");

    // The tool runs and the turn ends: nobody answered a permission.
    let output = wait_for(&mut events, |event| {
        matches!(event, AgentEvent::ToolOutput(_))
    })
    .await;
    assert!(output.is_some(), "the tool ran without asking");
    assert!(
        wait_for(&mut events, |event| {
            matches!(event, AgentEvent::TurnFinished(_))
        })
        .await
        .is_some(),
        "the turn ends"
    );
}

#[tokio::test]
async fn the_conversation_goes_on_from_turn_to_turn() {
    // The model sees what was said before: the second turn carries the first one.
    let runtime = a_runtime("seek-loop-history");
    let port = a_model_server().await;
    let (commands, mut events) = the_loop(Arc::clone(&runtime), port);

    commands
        .send(Command::Submit {
            text: "the first question".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");
    wait_for(&mut events, |event| {
        matches!(event, AgentEvent::TurnFinished(_))
    })
    .await;

    commands
        .send(Command::Submit {
            text: "the second question".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");
    wait_for(&mut events, |event| {
        matches!(event, AgentEvent::TurnFinished(_))
    })
    .await;

    let history = runtime.history.lock().await;
    let texts: Vec<String> = history.messages.iter().map(|m| m.plain_text()).collect();
    assert!(
        texts.iter().any(|text| text.contains("the first question")),
        "the first question is in the conversation: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("the second question")),
        "the second question is in the conversation: {texts:?}"
    );
}
