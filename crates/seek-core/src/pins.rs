//! The `@` file pins of a message (SPEC 9.5, D-019).
//!
//! The oh-my-pi harness reads the whole content of an `@file` mention into the conversation. That
//! is convenient but expensive: a five-megabyte file becomes five megabytes of prompt. SEEK pins
//! the path instead. The model gets a one-line description of each pin and inspects it with the
//! `read` tool when it needs to; the permission matrix still decides what it may do.
//!
//! A mention that does not resolve to an existing path is prose, not a pin.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// How many paths one message can pin.
pub const MAX_PINS: usize = 8;
/// How many names of a pinned directory are listed in the context.
pub const MAX_NAMES: usize = 12;

/// A path pinned by an `@` mention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// The text the user typed, without the `@`.
    pub label: String,
    /// The path the mention resolves to.
    pub path: PathBuf,
    /// Whether the path is a directory.
    pub is_dir: bool,
    /// The size of the file, in bytes (zero for a directory).
    pub size: u64,
}

/// Characters that may come before an `@` mention.
const BOUNDARY: [char; 10] = [' ', '\t', '\n', '(', '[', '{', '<', '"', '\'', '`'];
/// Characters that end a bare mention (a quoted mention ends at its quote instead).
const TRAILING: [char; 13] = [
    ',', '.', ';', ':', '!', '?', ')', ']', '}', '>', '"', '\'', '`',
];

/// The pins in a message, resolved against the project root.
pub fn find(text: &str, root: &Path) -> Vec<Pin> {
    let chars: Vec<char> = text.chars().collect();
    let mut pins = Vec::new();
    let mut seen = HashSet::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '@' || (index > 0 && !BOUNDARY.contains(&chars[index - 1])) {
            index += 1;
            continue;
        }
        let start = index + 1;
        if start >= chars.len() {
            break;
        }

        let (raw, next) = if matches!(chars[start], '"' | '\'') {
            let quote = chars[start];
            let mut end = start + 1;
            while end < chars.len() && chars[end] != quote {
                end += 1;
            }
            let raw: String = chars[start + 1..end].iter().collect();
            (raw, if end < chars.len() { end + 1 } else { end })
        } else {
            let mut end = start;
            while end < chars.len()
                && !chars[end].is_whitespace()
                && chars[end] != '@'
                && !matches!(chars[end], '(' | '[' | '{' | '<' | '"' | '\'' | '`')
            {
                end += 1;
            }
            (chars[start..end].iter().collect(), end)
        };
        index = next.max(index + 1);

        let label = raw
            .trim()
            .trim_matches(|c| TRAILING.contains(&c))
            .to_string();
        if label.is_empty() || label.starts_with('@') {
            continue;
        }
        let Some(path) = resolve(&label, root) else {
            continue;
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if seen.insert(path.clone()) {
            pins.push(Pin {
                label,
                path,
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }
        if pins.len() >= MAX_PINS {
            break;
        }
    }
    pins
}

/// Resolve a mention to a path that exists.
fn resolve(label: &str, root: &Path) -> Option<PathBuf> {
    let path = if let Some(rest) = label.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return None;
        }
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(label)
    };
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    path.exists().then_some(path)
}

/// Replace `@` mentions of existing paths with their absolute paths.
///
/// The pin context already lists the full path, but the user's own message still says
/// `@documents`. Expanding it in the text the model reads removes the guesswork about where the
/// file lives. The UI keeps the short form the user typed.
pub fn expand(text: &str, root: &Path) -> String {
    let mut out = text.to_string();
    for pin in find(text, root) {
        let absolute = pin.path.to_string_lossy();
        out = out
            .replace(&format!("@\"{}\"", pin.label), absolute.as_ref())
            .replace(&format!("@{}", pin.label), absolute.as_ref());
    }
    out
}

/// A pasted clipboard path (plain, `file://` or `~/`) that exists on disk.
pub fn pasted_path(text: &str, root: &Path) -> Option<PathBuf> {
    let trimmed = text.trim().trim_matches(['"', '\'']);
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }
    let raw = trimmed
        .strip_prefix("file://")
        .unwrap_or(trimmed)
        .replace("%20", " ");
    let path = if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").ok()?;
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(&raw)
    };
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    path.exists().then_some(path)
}

/// The compact context for a turn with pins.
///
/// Only paths, sizes and the first names of a directory go into the prompt: no file content. This
/// is the low-token counterpart of OMP's `FileMentionMessage` (MIT).
pub fn context(pins: &[Pin]) -> String {
    if pins.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "Pinned paths for this turn (inspect with `read`; permissions still apply):\n",
    );
    for pin in pins {
        if pin.is_dir {
            let names = directory_names(&pin.path);
            out.push_str(&format!(
                "- {} — {}: {}\n",
                pin.label,
                pin.path.display(),
                names.join(", ")
            ));
        } else {
            out.push_str(&format!(
                "- {} — {}, {}\n",
                pin.label,
                pin.path.display(),
                human_size(pin.size)
            ));
        }
    }
    out
}

/// The first names of a directory, sorted, directories marked with a slash.
fn directory_names(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| {
            let mut name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                name.push('/');
            }
            name
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.truncate(MAX_NAMES);
    if names.is_empty() {
        names.push("(empty)".to_string());
    }
    names
}

/// A size in words, with one decimal when it is not a whole unit.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value.fract() == 0.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "seek-pins-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::create_dir_all(root.join("docs/old")).expect("docs");
        std::fs::write(root.join("src/main.rs"), "the secret content").expect("file");
        std::fs::write(root.join("docs/GUIDE.md"), "guide").expect("guide");
        std::fs::write(root.join("two words.txt"), "spaces").expect("spaces");
        for index in 0..10 {
            std::fs::write(root.join(format!("src/f{index}.rs")), "").expect("file");
        }
        root
    }

    #[test]
    fn an_at_mention_pins_an_existing_path() {
        let root = project();
        let pins = find("look at @src/main.rs please", &root);
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].label, "src/main.rs");
        assert!(!pins[0].is_dir);
        assert_eq!(pins[0].path, root.join("src/main.rs"));
    }

    #[test]
    fn a_quoted_path_with_a_space_is_pinned() {
        let root = project();
        let pins = find("read @\"two words.txt\"", &root);
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].label, "two words.txt");
    }

    #[test]
    fn punctuation_and_emails_are_not_pins() {
        let root = project();
        let pins = find("email ada@example.com, then @src/main.rs.", &root);
        assert_eq!(pins.len(), 1, "{pins:?}");
        assert_eq!(pins[0].label, "src/main.rs");
    }

    #[test]
    fn a_missing_path_is_prose() {
        let root = project();
        assert!(find("read @src/missing.rs", &root).is_empty());
    }

    #[test]
    fn mentions_are_deduplicated_and_capped() {
        let root = project();
        let text = format!("{} ", "@src/main.rs ".repeat(MAX_PINS + 3));
        let pins = find(&text, &root);
        assert_eq!(pins.len(), 1, "the same path is pinned once");

        let text = (0..MAX_PINS + 2)
            .map(|index| format!("@src/f{index}.rs "))
            .collect::<String>();
        assert_eq!(
            find(&text, &root).len(),
            MAX_PINS,
            "one message pins at most eight paths"
        );
    }

    #[test]
    fn pasted_paths_are_normalized() {
        let root = project();
        let file = root.join("src/main.rs");
        assert_eq!(
            pasted_path(&format!("file://{}", file.display()), &root),
            Some(file.clone())
        );
        assert_eq!(pasted_path("src/main.rs", &root), Some(file));
        assert_eq!(pasted_path("no/such/file", &root), None);
    }

    #[test]
    fn mentions_expand_to_absolute_paths_for_the_model() {
        let root = project();
        let text = "read @src/main.rs and @docs";
        let expanded = expand(text, &root);
        assert_eq!(
            expanded,
            format!(
                "read {} and {}",
                root.join("src/main.rs").display(),
                root.join("docs").display()
            )
        );
    }

    #[test]
    fn the_context_is_compact_and_has_no_content() {
        let root = project();
        let pins = find("read @src/main.rs and @docs", &root);
        let text = context(&pins);
        assert!(
            text.contains(&format!(
                "src/main.rs — {}, 18 B",
                root.join("src/main.rs").display()
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!("docs — {}:", root.join("docs").display())),
            "{text}"
        );
        assert!(text.contains("old/"), "{text}");
        assert!(!text.contains("the secret content"), "{text}");
        assert!(
            text.encode_utf16().count() < 300,
            "two pins must stay under a few dozen tokens: {text}"
        );
    }
}
