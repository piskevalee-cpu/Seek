//! The completions of the prompt bar (SPEC 10.3).
//!
//! The composer completes what is typed in the message, the way the oh-my-pi composer does
//! (MIT): the slash commands and the skills at the start of the line, an `@` reference to a file
//! of the project, and a path (`src/`, `./`, `../`, `~/` or an absolute one). The list appears
//! above the bar; Tab takes the selected one, the arrows move, Esc closes it.
//!
//! A command replaces the line, because a command is the whole message. A path replaces only the
//! token under the cursor, so the prose around it stays: `read src/ma|` completes to
//! `read src/main.rs `.
//!
//! The names and the shape follow `docs/DECISIONS.md` D-017.

use std::path::{Path, PathBuf};

use crate::input::{Input, SLASH_COMMANDS};

/// How many completions are offered at once.
pub const MAX_ITEMS: usize = 50;

/// What a completion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A slash command or a skill.
    Command,
    /// A file.
    File,
    /// A directory; taking it opens the list of what is inside.
    Directory,
}

/// One completion the bar offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// The text that replaces the token under the cursor.
    pub value: String,
    /// The name shown in the list.
    pub label: String,
    /// The second column: the description of a command, the kind of a path.
    pub detail: String,
    /// What taking it does.
    pub kind: Kind,
}

/// What the cursor is completing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Context {
    /// A slash command at the start of the line.
    Command {
        /// The column of the `/`.
        start: usize,
        /// The text after the `/`, up to the cursor.
        prefix: String,
    },
    /// A path or an `@` reference.
    Path {
        /// The column where the token starts.
        start: usize,
        /// The path after the `@` (or after the quote).
        prefix: String,
        /// Whether the token is an `@` reference.
        at: bool,
        /// Whether the token started with a quote.
        quoted: bool,
    },
}

/// The characters that separate one token from the next.
const DELIMITERS: [char; 5] = [' ', '\t', '"', '\'', '='];

/// What the cursor is completing now, if anything.
pub fn context(input: &Input) -> Option<Context> {
    let (_, column) = input.cursor();
    let before: String = input.current_line().chars().take(column).collect();

    // The slash commands: the line is the command, no space yet.
    let trimmed = before.trim_start();
    if trimmed.starts_with('/') && !trimmed.contains(' ') {
        let start = before.chars().count() - trimmed.chars().count();
        return Some(Context::Command {
            start,
            prefix: trimmed.chars().skip(1).collect(),
        });
    }

    // The token under the cursor.
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| DELIMITERS.contains(c))
        .map(|(index, c)| index + c.len_utf8())
        .unwrap_or(0);
    let token = &before[start..];
    let start = before[..start].chars().count();

    if let Some(rest) = token.strip_prefix("@\"") {
        return Some(Context::Path {
            start,
            prefix: rest.to_string(),
            at: true,
            quoted: true,
        });
    }
    if let Some(rest) = token.strip_prefix('@') {
        return Some(Context::Path {
            start,
            prefix: rest.to_string(),
            at: true,
            quoted: false,
        });
    }
    if let Some(rest) = token.strip_prefix('"') {
        return Some(Context::Path {
            start,
            prefix: rest.to_string(),
            at: false,
            quoted: true,
        });
    }
    if token.starts_with('/')
        || token.starts_with('.')
        || token.starts_with("~/")
        || token.contains('/')
    {
        return Some(Context::Path {
            start,
            prefix: token.to_string(),
            at: false,
            quoted: false,
        });
    }
    None
}

/// The completions for what the cursor is doing.
pub fn completions(input: &Input, skills: &[seek_core::Skill], cwd: &Path) -> Vec<Completion> {
    match context(input) {
        Some(Context::Command { prefix, .. }) => {
            let commands = command_completions(&prefix, skills);
            if commands.is_empty() && prefix.contains('/') {
                // A path at the start of the line, not a command: `/tmp/fo`.
                return path_completions(&format!("/{prefix}"), cwd, false, false);
            }
            commands
        }
        Some(Context::Path {
            prefix, at, quoted, ..
        }) => path_completions(&prefix, cwd, at, quoted),
        None => Vec::new(),
    }
}

/// The slash commands and the skills that start with `prefix`.
fn command_completions(prefix: &str, skills: &[seek_core::Skill]) -> Vec<Completion> {
    let needle = prefix.to_lowercase();
    let mut items: Vec<Completion> = SLASH_COMMANDS
        .iter()
        .filter(|(command, _)| {
            command
                .trim_start_matches('/')
                .to_lowercase()
                .starts_with(&needle)
        })
        .map(|(command, description)| Completion {
            value: (*command).to_string(),
            label: (*command).to_string(),
            detail: (*description).to_string(),
            kind: Kind::Command,
        })
        .collect();
    for skill in skills {
        let command = format!("/{}", skill.name);
        if command
            .trim_start_matches('/')
            .to_lowercase()
            .starts_with(&needle)
            && !items.iter().any(|item| item.value == command)
        {
            items.push(Completion {
                value: command.clone(),
                label: command,
                detail: skill.description.clone(),
                kind: Kind::Command,
            });
        }
    }
    items
}

/// The paths under `prefix`.
fn path_completions(prefix: &str, cwd: &Path, at: bool, quoted: bool) -> Vec<Completion> {
    let (dir_part, base) = match prefix.rfind('/') {
        Some(index) => (&prefix[..=index], &prefix[index + 1..]),
        None => ("", prefix),
    };

    // An `@` reference with no folder searches the whole project; everything else lists one
    // directory, the way a shell does.
    if at && dir_part.is_empty() && !base.is_empty() {
        return project_matches(base, cwd, at, quoted);
    }

    let directory = resolve_dir(cwd, dir_part);
    let mut entries = read_dir(&directory, base);
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    entries
        .into_iter()
        .take(MAX_ITEMS)
        .map(|(is_dir, name)| {
            let display = format!("{dir_part}{name}");
            let location = directory.join(name).to_string_lossy().to_string();
            completion_value(&display, is_dir, at, quoted, &location)
        })
        .collect()
}

/// The entries of a directory whose name starts with `base`.
fn read_dir(directory: &Path, base: &str) -> Vec<(bool, String)> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let needle = base.to_lowercase();
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && !base.starts_with('.') {
                return None;
            }
            if !name.to_lowercase().starts_with(&needle) {
                return None;
            }
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            Some((is_dir, name))
        })
        .collect()
}

/// The files of the project that match an `@` reference.
fn project_matches(base: &str, cwd: &Path, at: bool, quoted: bool) -> Vec<Completion> {
    let needle = base.to_lowercase();
    let mut matches: Vec<(u32, Completion)> = Vec::new();
    let mut seen = 0usize;

    let walk = ignore::WalkBuilder::new(cwd)
        .hidden(false)
        .git_ignore(true)
        .max_depth(Some(8))
        .build();

    for entry in walk.filter_map(Result::ok) {
        seen += 1;
        if seen > 4_000 || matches.len() >= MAX_ITEMS {
            break;
        }
        let path = entry.path();
        if path == cwd {
            continue;
        }
        let Some(relative) = path.strip_prefix(cwd).ok() else {
            continue;
        };
        let display = relative.to_string_lossy().replace('\\', "/");
        // The repository's own metadata is never a useful file reference.
        if display == ".git" || display.starts_with(".git/") {
            continue;
        }
        let Some(score) = fuzzy_score(&needle, &display) else {
            continue;
        };
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        let location = cwd.join(&display).to_string_lossy().to_string();
        matches.push((
            score,
            completion_value(&display, is_dir, at, quoted, &location),
        ));
    }

    // The tightest match first, then the directories before the files, then the name.
    matches.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| (b.1.kind == Kind::Directory).cmp(&(a.1.kind == Kind::Directory)))
            .then_with(|| a.1.label.cmp(&b.1.label))
    });
    matches.into_iter().map(|(_, item)| item).collect()
}

/// How well a path matches a query: a prefix of a name beats a substring, which beats a
/// subsequence.
fn fuzzy_score(query: &str, display: &str) -> Option<u32> {
    let display = display.to_lowercase();
    let name = display.rsplit('/').next().unwrap_or(&display);
    if name == query || display == query {
        return Some(100);
    }
    if name.starts_with(query) {
        return Some(80);
    }
    if display.contains(query) {
        return Some(60);
    }
    let mut chars = query.chars();
    let mut wanted = chars.next();
    let mut gaps = 0u32;
    let mut matched = false;
    for c in display.chars() {
        match wanted {
            Some(w) if w == c => {
                wanted = chars.next();
                matched = true;
            }
            Some(_) => {
                if matched {
                    gaps += 1;
                }
            }
            None => break,
        }
    }
    if wanted.is_some() {
        return None;
    }
    Some(40u32.saturating_sub(gaps.saturating_mul(5)).max(1))
}

/// The text a completion puts in place of the token.
fn completion_value(
    display: &str,
    is_dir: bool,
    at: bool,
    quoted: bool,
    location: &str,
) -> Completion {
    let mut text = display.to_string();
    if is_dir {
        text.push('/');
    }
    let marker = if at { "@" } else { "" };
    let value = if quoted || display.contains(' ') {
        format!("{marker}\"{text}\"")
    } else {
        format!("{marker}{text}")
    };
    // A file ends the token; a directory does not, so the list can go on inside it.
    let value = if is_dir { value } else { format!("{value} ") };
    Completion {
        value,
        label: text,
        detail: location.to_string(),
        kind: if is_dir { Kind::Directory } else { Kind::File },
    }
}

/// The folder a path prefix points at.
fn resolve_dir(cwd: &Path, dir_part: &str) -> PathBuf {
    let expanded = if let Some(rest) = dir_part.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/{rest}")
    } else {
        dir_part.to_string()
    };
    let path = Path::new(&expanded);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "seek-completion-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::create_dir_all(root.join(".hidden")).expect("hidden");
        std::fs::write(root.join("src/main.rs"), "").expect("main");
        std::fs::write(root.join("src/model.rs"), "").expect("model");
        std::fs::write(root.join("README.md"), "").expect("readme");
        root
    }

    fn typed(text: &str) -> Input {
        let mut input = Input::new();
        for c in text.chars() {
            input.insert_char(c);
        }
        input
    }

    #[test]
    fn the_commands_and_the_skills_are_completed() {
        let skills = vec![seek_core::Skill {
            name: "init".into(),
            description: "Start a project".into(),
            path: PathBuf::new(),
            body: String::new(),
            allowed_tools: Vec::new(),
            license: None,
            compatibility: None,
            metadata: None,
        }];
        let items = completions(&typed("/mod"), &skills, Path::new("/tmp"));
        assert!(items.iter().any(|item| item.value == "/model"));
        assert!(items.iter().all(|item| item.kind == Kind::Command));

        let items = completions(&typed("/i"), &skills, Path::new("/tmp"));
        assert!(items.iter().any(|item| item.value == "/init"));
    }

    #[test]
    fn a_path_completes_inside_the_project() {
        let root = project();
        let items = completions(&typed("read src/ma"), &[], &root);
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].value, "src/main.rs ");
        assert_eq!(items[0].kind, Kind::File);
        assert_eq!(items[0].detail, root.join("src/main.rs").to_string_lossy());
    }

    #[test]
    fn a_directory_keeps_the_token_open() {
        let root = project();
        let items = completions(&typed("@src"), &[], &root);
        assert!(
            items
                .iter()
                .any(|item| item.value == "@src/" && item.kind == Kind::Directory),
            "{items:?}"
        );
    }

    #[test]
    fn an_at_reference_searches_the_whole_project() {
        let root = project();
        let items = completions(&typed("look at @model"), &[], &root);
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].value, "@src/model.rs ");
    }

    #[test]
    fn hidden_files_need_a_dot_in_a_path() {
        let root = project();
        let paths = completions(&typed("open .h"), &[], &root);
        assert!(
            paths.iter().any(|item| item.label == ".hidden/"),
            "{paths:?}"
        );
        let at = completions(&typed("@hid"), &[], &root);
        assert!(
            at.iter().any(|item| item.label.contains(".hidden")),
            "{at:?}"
        );
    }

    #[test]
    fn prose_is_not_completed() {
        let root = project();
        assert!(completions(&typed("read the file"), &[], &root).is_empty());
    }

    #[test]
    fn context_marks_where_the_token_starts() {
        let input = typed("read src/ma");
        match context(&input) {
            Some(Context::Path { start, prefix, .. }) => {
                assert_eq!(start, 5);
                assert_eq!(prefix, "src/ma");
            }
            other => panic!("expected a path context, got {other:?}"),
        }
    }

    #[test]
    fn a_file_is_quoted_when_it_has_a_space() {
        let root = project();
        std::fs::write(root.join("two words.txt"), "").expect("file");
        let items = completions(&typed("@two"), &[], &root);
        assert_eq!(items[0].value, "@\"two words.txt\" ");
    }
}
