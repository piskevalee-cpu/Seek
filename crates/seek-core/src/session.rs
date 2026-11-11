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

/// A session in the list of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// The id of the session; the name of its file.
    pub id: String,
    /// The title: the first user message.
    pub title: String,
    /// When the session was created.
    pub created: String,
}

impl SessionInfo {
    /// How old the session is, in a form a person reads.
    pub fn age(&self, now: std::time::SystemTime) -> String {
        let created = match parse_time(&self.created) {
            Some(created) => created,
            None => return "unknown".to_string(),
        };
        let seconds = now
            .duration_since(created)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        if seconds >= 86_400 {
            format!("{}d", seconds / 86_400)
        } else if seconds >= 3_600 {
            format!("{}h", seconds / 3_600)
        } else if seconds >= 60 {
            format!("{}m", seconds / 60)
        } else {
            "now".to_string()
        }
    }
}

/// Parse a timestamp of the format `2026-11-11T12:00:00Z`.
fn parse_time(text: &str) -> Option<std::time::SystemTime> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;
    let hour: u32 = text.get(11..13)?.parse().ok()?;
    let minute: u32 = text.get(14..16)?.parse().ok()?;
    let second: u32 = text.get(17..19)?.parse().ok()?;

    // The days since the epoch, with the leap years of the Gregorian calendar.
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    let leap = is_leap(year);
    let months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for length in months.iter().take(month as usize - 1) {
        days += *length as i64;
    }
    days += day as i64 - 1;
    let seconds_total = days * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
    if seconds_total < 0 {
        return None;
    }
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds_total as u64))
}

/// Whether a year is a leap year.
fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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
    ///
    /// The title is the first user message. It is not written back into the header, so that the
    /// file only ever grows.
    pub fn title(&self) -> String {
        if !self.header.title.is_empty() {
            return self.header.title.clone();
        }
        self.events()
            .unwrap_or_default()
            .into_iter()
            .find_map(|event| match event {
                SessionEvent::Message { message, .. }
                    if message.role == crate::message::Role::User =>
                {
                    Some(title_of(&message.plain_text()))
                }
                _ => None,
            })
            .unwrap_or_default()
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

    /// The sessions of a folder, the newest first.
    pub fn list(dir: &Path) -> Vec<SessionInfo> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut sessions: Vec<SessionInfo> = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
            .filter_map(|entry| {
                let path = entry.path();
                let session = Session::open(&path).ok()?;
                Some(SessionInfo {
                    id: session.header().id.clone(),
                    title: session.title(),
                    created: session.header().created.clone(),
                })
            })
            .collect();
        // The newest first.
        sessions.sort_by(|a, b| b.created.cmp(&a.created));
        sessions
    }

    /// The time of now, as the header of a session carries it.
    pub fn now() -> String {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        format_time(seconds)
    }

    /// An id for a new session.
    pub fn new_id() -> String {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        format!("session-{}", format_time(seconds).replace(':', "-"))
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

/// Format a number of seconds since the epoch as `2026-11-11T12:00:00Z`.
fn format_time(seconds: u64) -> String {
    let seconds = seconds as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);

    let mut year = 1970;
    let mut days_left = days;
    loop {
        let length = if is_leap(year) { 366 } else { 365 };
        if days_left < length {
            break;
        }
        days_left -= length;
        year += 1;
    }
    let leap = is_leap(year);
    let months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while month < 12 && days_left >= months[month] as i64 {
        days_left -= months[month] as i64;
        month += 1;
    }
    format!(
        "{year:04}-{:02}-{:02}T{hour:02}:{minute:02}:{second:02}Z",
        month + 1,
        days_left + 1
    )
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
    fn the_sessions_are_listed_with_their_title() {
        let dir = temp_dir("list");
        let mut session =
            Session::create(&dir, "/tmp", "2026-11-11T12:00:00Z", "one").expect("created");
        session
            .append_message(&Message::user("the first question"), None)
            .expect("appended");

        let sessions = Session::list(&dir);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "one");
        assert_eq!(sessions[0].title, "the first question");
        assert_eq!(session.title(), "the first question");
        // The age is a short duration, not "unknown": the timestamp parsed.
        assert_ne!(sessions[0].age(std::time::SystemTime::now()), "unknown");
    }

    #[test]
    fn the_time_round_trips() {
        let now = Session::now();
        let parsed = parse_time(&now).expect("parses");
        let seconds = parsed
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs();
        assert_eq!(format_time(seconds), now);
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
