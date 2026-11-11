//! The supervisor against a real llama.cpp server.
//!
//! The test needs a `llama` binary and a GGUF model. When they are not there, the test skips
//! itself, so the suite stays green on a machine without them.

use std::path::PathBuf;

use seek_local::model::scan;
use seek_local::options::LaunchOptions;
use seek_local::supervisor::Supervisor;

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
        if which(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Whether a program is on the PATH.
fn which(program: &str) -> bool {
    let Ok(paths) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|directory| directory.join(program).is_file())
}

#[tokio::test]
async fn the_supervisor_starts_a_server_and_reads_its_properties() {
    let (Some(binary), Some(model_path)) = (the_binary(), a_model()) else {
        eprintln!("no llama binary or no model: the test skips itself");
        return;
    };

    // The model folder of a fake home.
    let home = std::env::temp_dir().join("seek-supervisor-test");
    let _ = std::fs::remove_dir_all(&home);
    let models = home.join("models");
    std::fs::create_dir_all(&models).expect("the folder is created");
    let name = model_path.file_name().expect("the model has a name");
    std::fs::copy(&model_path, models.join(name)).expect("the model is copied");

    let local = scan(&models);
    assert_eq!(local.len(), 1, "the model is found");
    let model = &local[0];

    let mut supervisor = Supervisor::new(binary, Some("serve".into()), home.join("state"));
    // A small context: the KV cache of the default context of the model is several gigabytes.
    let options = LaunchOptions::parse("--ctx-size 4096");
    let port = 8321;

    supervisor
        .start(model, &options, port)
        .await
        .expect("the server starts");

    assert!(supervisor.is_running());
    assert_eq!(supervisor.port(), Some(port));

    // The real context size comes from /props, not from the flag.
    let props = supervisor.props().expect("the properties are read");
    assert!(
        props.n_ctx > 0,
        "the context size is known: {}",
        props.n_ctx
    );
    assert_eq!(props.total_slots, 1, "one slot, because --parallel 1");
    assert!(props.model_path.ends_with(name.to_string_lossy().as_ref()));

    // The reasoning support comes from the capabilities, not from the template text.
    assert!(props.supports_enable_thinking || !props.supports_reasoning_effort);

    // The token count of a conversation goes through /apply-template and /tokenize.
    let messages = vec![serde_json::json!({
        "role": "user",
        "content": "hello"
    })];
    let tokens = supervisor.count_tokens(&messages).await;
    assert!(tokens.is_some(), "the tokens are counted");
    assert!(tokens.unwrap() > 0);

    supervisor.stop().await;
    assert!(!supervisor.is_running());
}

#[tokio::test]
async fn the_supervisor_stops_the_server_it_started() {
    let (Some(binary), Some(model_path)) = (the_binary(), a_model()) else {
        eprintln!("no llama binary or no model: the test skips itself");
        return;
    };

    let home = std::env::temp_dir().join("seek-supervisor-stop-test");
    let _ = std::fs::remove_dir_all(&home);
    let models = home.join("models");
    std::fs::create_dir_all(&models).expect("the folder is created");
    let name = model_path.file_name().expect("the model has a name");
    std::fs::copy(&model_path, models.join(name)).expect("the model is copied");

    let local = scan(&models);
    let model = &local[0];

    let mut supervisor = Supervisor::new(binary, Some("serve".into()), home.join("state"));
    // A small context: the KV cache of the default context of the model is several gigabytes.
    let options = LaunchOptions::parse("--ctx-size 4096");
    supervisor
        .start(model, &options, 8322)
        .await
        .expect("the server starts");

    supervisor.stop().await;

    // The port is free again.
    assert!(seek_local::port::is_free(8322));
}
