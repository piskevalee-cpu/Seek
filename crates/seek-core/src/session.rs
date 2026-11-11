//! Sessions (SPEC 11).
//!
//! One append-only JSONL file per session in `state/sessions/<id>.jsonl`: a header with the id,
//! the working directory, the creation time and the title, then one line per message and per
//! model switch.
//!
//! Model switches are recorded as events, so a resumed session shows which model said what.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::message::{History, Message};

/// How long a title may be.
const TITLE_LENGTH: usize = 60;

/// The first line of a session file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHeader {
    pub id: String,
    pub cwd: String,
    /// The creation time, as an RFC 3339 timestamp.
    pub created: String,
    /// The first user message, truncated.
    pub title: String,
}

/// Which model produced a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelTag {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub local: bool,
}

/// One line of a session file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    /// A message, with the model that produced it.
    Message {
        #[serde(default)]
        model: Option<ModelTag>,
        message: Message,
    },
    /// The user switched models.
    ModelSwitch { model: ModelTag },
}

/// A session on disk.
#[derive(Debug, Clone)]
pub struct Session {
    path: PathBuf,
    header: SessionHeader,
}

impl Session {
    /// Start a new session in `dir`.
    ///
    /// The file is created with its header; nothing is overwritten, because the id is unique.
    pub fn create(dir: &Path, cwd: &str, created: &str, id: &str) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let header = SessionHeader {
            id: id.to_string(),
            cwd: cwd.to_string(),
            created: created.to_string(),
            title: String::new(),
        };
        let path = dir.join(format!("{id}.jsonl"));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        writeln!(file, "{}", serde_json::to_string(&header)?)?;
        Ok(Session { path, header })
    }

    /// Open an existing session.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let mut lines = BufReader::new(file).lines();
        let first = lines
            .next()
            .transpose()?
            .ok_or_else(|| std::io::Error::other("the session file is empty"))?;
        let header: SessionHeader = serde_json::from_str(&first)?;
        Ok(Session {
            path: path.to_path_buf(),
            header,
        })
    }

    /// The file the session lives in.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The header of the session.
    pub fn header(&self) -> &SessionHeader {
        &self.header
    }

    /// The title of the session.
    pub fn title(&self) -> &str {
        &self.header.title
    }

    /// Append a message.
    ///
    /// The first user message becomes the title of the session.
    pub fn append_message(
        &mut self,
        message: &Message,
        model: Option<&ModelTag>,
    ) -> std::io::Result<()> {
        if self.header.title.is_empty() && message.role == crate::message::Role::User {
            self.header.title = title_of(&message.plain_text());
        }
        let event = SessionEvent::Message {
            model: model.cloned(),
            message: message.clone(),
        };
        self.append(&event)
    }

    /// Append a model switch.
    pub fn append_model_switch(&mut self, model: &ModelTag) -> std::io::Result<()> {
        self.append(&SessionEvent::ModelSwitch {
            model: model.clone(),
        })
    }

    /// Append one line.
    fn append(&mut self, event: &SessionEvent) -> std::io::Result<()> {
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(event)?)?;
        file.flush()
    }

    /// Read every event of the session.
    pub fn events(&self) -> std::io::Result<Vec<SessionEvent>> {
        let file = File::open(&self.path)?;
        let mut events = Vec::new();
        for line in BufReader::new(file).lines().skip(1) {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<SessionEvent>(&line) {
                Ok(event) => events.push(event),
                // A line that cannot be read is skipped, so a damaged file still opens.
                Err(_) => continue,
            }
        }
        Ok(events)
    }

    /// Replay the session into the canonical model.
    pub fn replay(&self) -> std::io::Result<History> {
        let mut history = History::new();
        for event in self.events()? {
            if let SessionEvent::Message { message, .. } = event {
                history.push(message);
            }
        }
        Ok(history)
    }

    /// The model that was active for each message, in order.
    ///
    /// A model switch changes the model for everything that follows.
    pub fn model_history(&self) -> std::io::Result<Vec<Option<ModelTag>>> {
        let mut out = Vec::new();
        let mut current: Option<ModelTag> = None;
        for event in self.events()? {
            match event {
                SessionEvent::ModelSwitch { model } => current = Some(model),
                SessionEvent::Message { model, .. } => {
                    out.push(model.or_else(|| current.clone()));
                }
            }
        }
        Ok(out)
    }
}

/// The title of a session: the first user message, truncated.
fn title_of(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= TITLE_LENGTH {
        return trimmed.to_string();
    }
    let mut title: String = trimmed.chars().take(TITLE_LENGTH).collect();
    title.push('…');
    title
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Block, Role};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-session-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_session_is_written_and_replayed() {
        let dir = temp_dir("replay");
        let mut session =
            Session::create(&dir, "/tmp/project", "2026-11-11T12:00:00Z", "session-1")
                .expect("the session is created");

        session
            .append_message(&Message::user("hello"), None)
            .expect("the message is appended");
        session
            .append_message(&Message::assistant("hi"), None)
            .expect("the message is appended");

        let reopened = Session::open(session.path()).expect("the session opens");
        let history = reopened.replay().expect("the session replays");
        assert_eq!(history.messages.len(), 2);
        assert_eq!(history.messages[0].plain_text(), "hello");
        assert_eq!(history.messages[1].plain_text(), "hi");
    }

    #[test]
    fn the_title_is_the_first_user_message() {
        let dir = temp_dir("title");
        let mut session =
            Session::create(&dir, "/tmp", "2026-11-11T12:00:00Z", "session-2").expect("created");
        session
            .append_message(&Message::user("  add a test  "), None)
            .expect("appended");
        assert_eq!(session.title(), "add a test");
    }

    #[test]
    fn a_long_title_is_truncated() {
        let dir = temp_dir("long-title");
        let mut session =
            Session::create(&dir, "/tmp", "2026-11-11T12:00:00Z", "session-3").expect("created");
        let long = "x".repeat(200);
        session
            .append_message(&Message::user(&long), None)
            .expect("appended");
        assert!(session.title().chars().count() <= TITLE_LENGTH + 1);
    }

    #[test]
    fn a_model_switch_is_recorded() {
        let dir = temp_dir("switch");
        let mut session =
            Session::create(&dir, "/tmp", "2026-11-11T12:00:00Z", "session-4").expect("created");
        let model = ModelTag {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-5".into(),
            local: false,
        };
        session
            .append_message(&Message::user("hi"), Some(&model))
            .expect("appended");
        session.append_model_switch(&model).expect("appended");
        session
            .append_message(&Message::assistant("hello"), None)
            .expect("appended");

        let history = session.model_history().expect("readable");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].as_ref().unwrap().model, "claude-sonnet-4-5");
        assert_eq!(history[1].as_ref().unwrap().model, "claude-sonnet-4-5");
    }

    #[test]
    fn the_file_is_append_only() {
        let dir = temp_dir("append");
        let mut session =
            Session::create(&dir, "/tmp", "2026-11-11T12:00:00Z", "session-5").expect("created");
        session
            .append_message(&Message::user("one"), None)
            .expect("appended");
        let first = fs::read_to_string(session.path()).expect("readable");
        session
            .append_message(&Message::user("two"), None)
            .expect("appended");
        let second = fs::read_to_string(session.path()).expect("readable");
        assert!(second.starts_with(&first));
        assert_eq!(second.lines().count(), 3);
    }

    #[test]
    fn thinking_blocks_survive_a_round_trip() {
        let dir = temp_dir("thinking");
        let mut session =
            Session::create(&dir, "/tmp", "2026-11-11T12:00:00Z", "session-6").expect("created");
        let message = Message::new(
            Role::Assistant,
            vec![Block::thinking("anthropic", "hmm"), Block::text("done")],
        );
        session.append_message(&message, None).expect("appended");
        let history = session.replay().expect("replayed");
        assert_eq!(history.messages[0].blocks.len(), 2);
    }
}
