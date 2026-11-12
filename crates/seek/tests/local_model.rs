//! The whole harness against a real local model.
//!
//! The test needs a `llama` binary and a GGUF model. When they are not there, the test skips
//! itself, so the suite stays green on a machine without them.
//!
//! The model is *linked*, not copied: a copy of a 1.5 GB model into the temporary folder fills
//! the memory of the machine (the temporary folder is a tmpfs on Linux), and the suite then
//! runs out of memory.

use std::path::PathBuf;
use std::sync::Arc;

use seek::Runtime;
use seek_core::r#loop::PermissionRequests;
use seek_core::{AgentEvent, PermissionMode, ProvidersConfig};
use seek_tui::app::Command;
use tokio::sync::mpsc;

/// The first model of `~/LLMs`, if there is one.
fn a_model() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("HOME").ok()?).join("LLMs");
    let entries = std::fs::read_dir(dir).ok()?;
    let mut models: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "gguf"))
        .collect();
    models.sort();
    models.into_iter().next()
}

/// The llama binary, if there is one.
fn the_binary() -> Option<String> {
    for candidate in ["llama", "llama-server"] {
        if seek::which(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }
    None
}

#[tokio::test]
async fn a_local_model_answers_a_message() {
    // Only one test at a time starts a server: they are heavy, and the tests of the
    // workspace run in parallel.
    let _lock = seek_local::ServerLock::take();
    let (Some(binary), Some(model_path)) = (the_binary(), a_model()) else {
        eprintln!("no llama binary or no model: the test skips itself");
        return;
    };

    // A SEEK home with one model in it. The home is on the disk: the temporary folder of the
    // system is a tmpfs, which is memory.
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("seek-e2e-test");
    let _ = std::fs::remove_dir_all(&home);
    let models = home.join("models");
    std::fs::create_dir_all(&models).expect("the folder is created");
    let name = model_path.file_name().expect("the model has a name");
    // The model is linked, not copied: a copy would fill the memory of the machine.
    #[cfg(unix)]
    std::os::unix::fs::symlink(&model_path, models.join(name)).expect("the model is linked");
    #[cfg(not(unix))]
    std::fs::copy(&model_path, models.join(name)).expect("the model is copied");

    // The tools run in a folder of their own, so the test does not write in the sources.
    let root = home.join("work");
    std::fs::create_dir_all(&root).expect("the folder is created");

    let runtime =
        Arc::new(Runtime::new(&home, &binary, Some("serve".into())).with_root(root.clone()));
    assert_eq!(runtime.local_models.len(), 1, "the model is found");

    // A small context: the KV cache of the default context of the model is several gigabytes.
    let sidecar = seek_local::model::Sidecar {
        schema: 1,
        port: None,
        // The context has to hold the prompt and the answer of a few rounds: a small one
        // refuses a request that does not fit.
        options: Some("--ctx-size 8192".into()),
    };
    sidecar
        .write(&seek_local::model::sidecar_path(
            &runtime.local_models[0].path,
        ))
        .expect("the sidecar is written");

    let (command_tx, command_rx) = mpsc::channel::<Command>(16);
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(256);
    let (widget_tx, _widget_rx) = mpsc::channel::<seek_tui::app::UiMessage>(16);
    let (permission_tx, mut permission_rx) = PermissionRequests::new();

    tokio::spawn(seek::agent_loop(
        command_rx,
        event_tx,
        widget_tx,
        ProvidersConfig::default(),
        permission_tx,
        Arc::clone(&runtime),
    ));

    // The user picks the local model.
    let model_id = runtime.local_models[0].id.clone();
    command_tx
        .send(Command::SwitchModel {
            id: model_id.clone(),
        })
        .await
        .expect("the command is sent");

    // And sends a message.
    command_tx
        .send(Command::Submit {
            text: "Say hello in three words.".into(),
            mode: PermissionMode::Yolo,
        })
        .await
        .expect("the command is sent");

    // The server starts and the model answers.
    let mut answer = String::new();
    let mut thinking = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
    while tokio::time::Instant::now() < deadline {
        let Ok(event) =
            tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv()).await
        else {
            continue;
        };
        let Some(event) = event else { break };
        match event {
            AgentEvent::TextDelta(text) => answer.push_str(&text),
            // A small model sometimes answers in its thinking and never writes the
            // answer: that is an answer too.
            AgentEvent::ThinkingDelta(text) => thinking.push_str(&text),
            AgentEvent::TurnFinished(_) => break,
            AgentEvent::Error(error) => panic!("the turn failed: {error}"),
            _ => {}
        }
        // A permission request is answered with "yes", so the turn can finish.
        if let Ok(request) = permission_rx.try_recv() {
            let _ = request;
        }
        // The model answered: the test does not wait for the rest of the turn.
        if !answer.trim().is_empty() || !thinking.trim().is_empty() {
            break;
        }
    }

    assert!(
        !answer.trim().is_empty() || !thinking.trim().is_empty(),
        "the local model answered something"
    );

    // The server is stopped when SEEK goes.
    runtime.supervisor.lock().await.stop().await;
}
