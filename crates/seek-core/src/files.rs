//! The file tools: `read`, `write` and `edit` (SPEC 9.2).
//!
//! The behaviour follows the grok-build pager (Apache-2.0):
//!
//! - **The read tool numbers the lines.** The first line and every tenth line carry an anchor
//!   (`1→`, `11→`, `21→`), the lines in between carry their content only. The model can point
//!   at a line, and it knows the anchors are not part of the file.
//! - **The write tool creates the file**, and the folders above it.
//! - **The edit tool replaces an exact string.** The match has to be unique, unless
//!   `replace_all` is set. The answer says which line changed and shows the difference, so the
//!   model sees what it did.
//!
//! The names stay `read`, `write` and `edit`: the specification and the saved sessions use
//! them.

use std::path::{Path, PathBuf};

use crate::tool::{Risk, ToolError, ToolSchema};
use crate::truncate::cap_output;

/// Every tenth line carries an anchor, and so does the first one.
pub const ANCHOR_EVERY: usize = 10;

/// The separator between the line number and the content.
pub const ANCHOR: char = '→';

/// The most lines a read gives back.
pub const MAX_LINES: usize = 200;

/// Reads a file, with the line numbers.
#[derive(Debug, Clone)]
pub struct ReadTool {
    root: PathBuf,
}

impl ReadTool {
    pub fn new(root: PathBuf) -> Self {
        ReadTool { root }
    }

    /// Read the file, honouring offset and limit.
    ///
    /// The lines carry the anchors: the first one of the window and every tenth. The model can
    /// point at a line, and the edit tool can say where the change landed.
    pub fn read(
        &self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<String, ToolError> {
        let resolved = resolve(&self.root, path);
        let text = std::fs::read_to_string(&resolved)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;

        let all: Vec<&str> = text.lines().collect();
        let start = offset.unwrap_or(1).max(1) - 1;
        if start >= all.len() && !all.is_empty() {
            return Ok(format!(
                "the file has {} lines, offset {start} is past the end",
                all.len()
            ));
        }
        let take = limit.unwrap_or(MAX_LINES).min(MAX_LINES);
        let slice: Vec<&str> = all[start..(start + take).min(all.len())].to_vec();

        // The anchors: the first line of the window and every tenth line of the file.
        let mut out = String::new();
        for (index, line) in slice.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            let number = start + index + 1;
            if index == 0 || number.is_multiple_of(ANCHOR_EVERY) {
                out.push_str(&format!("{number}{ANCHOR}{line}"));
            } else {
                out.push_str(line);
            }
        }
        if start + take < all.len() {
            out.push_str(&format!(
                "\n… [{} more lines, use offset {} to see them] …",
                all.len() - start - take,
                start + take + 1
            ));
        }
        Ok(cap_output(&out))
    }
}

/// Writes a file, creating it when it is not there.
#[derive(Debug, Clone)]
pub struct WriteTool {
    root: PathBuf,
}

impl WriteTool {
    pub fn new(root: PathBuf) -> Self {
        WriteTool { root }
    }

    /// Write the file, replacing its content.
    pub fn write(&self, path: &str, content: &str) -> Result<String, ToolError> {
        let resolved = resolve(&self.root, path);
        let existed = resolved.exists();
        if let Some(parent) = resolved.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| ToolError::Failed(format!("{}: {error}", parent.display())))?;
        }
        std::fs::write(&resolved, content)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;
        let lines = content.lines().count();
        Ok(if existed {
            format!(
                "The file {} has been overwritten. It has {lines} lines.",
                resolved.display()
            )
        } else {
            format!(
                "The file {} has been created. It has {lines} lines.",
                resolved.display()
            )
        })
    }
}

/// Replaces an exact string in a file.
#[derive(Debug, Clone)]
pub struct EditTool {
    root: PathBuf,
}

impl EditTool {
    pub fn new(root: PathBuf) -> Self {
        EditTool { root }
    }

    /// Replace `old` with `new`.
    ///
    /// The match has to be unique, unless `replace_all` is set. The answer says which line
    /// changed and shows the difference: the model sees what it did, and it can correct it.
    pub fn edit(
        &self,
        path: &str,
        old: &str,
        new: &str,
        replace_all: bool,
    ) -> Result<String, ToolError> {
        let resolved = resolve(&self.root, path);

        // An empty `old` creates the file: that is how a new file is written.
        if old.is_empty() {
            if resolved.exists() {
                return Err(ToolError::Failed(format!(
                    "{} already exists; give the text to replace, or use the write tool",
                    resolved.display()
                )));
            }
            return WriteTool::new(self.root.clone()).write(path, new);
        }

        let text = std::fs::read_to_string(&resolved)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;

        let matches = text.matches(old).count();
        match (matches, replace_all) {
            (0, _) => {
                return Err(ToolError::Failed(format!(
                    "the text to replace was not found in {}. Read the file again: the line \
                     numbers of the read are anchors, not part of the file.",
                    resolved.display()
                )));
            }
            (1, _) => {}
            (count, false) => {
                return Err(ToolError::Failed(format!(
                    "the text to replace is not unique: {count} matches in {}. Add the lines \
                     around it, or set replace_all to change every occurrence.",
                    resolved.display()
                )));
            }
            _ => {}
        }

        let updated = if replace_all {
            text.replace(old, new)
        } else {
            text.replacen(old, new, 1)
        };
        std::fs::write(&resolved, &updated)
            .map_err(|error| ToolError::Failed(format!("{}: {error}", resolved.display())))?;

        // The answer says where the change landed, and shows the difference.
        let old_line = line_of(&text, old);
        let new_line =
            old_line.map(|line| line + new.lines().count().saturating_sub(old.lines().count()));
        let mut out = format!(
            "The file {} has been updated{}.",
            resolved.display(),
            if replace_all && matches > 1 {
                ". All occurrences were replaced"
            } else {
                ""
            }
        );
        if let Some(line) = old_line {
            out.push_str(&format!("\nThe change was at line {line}."));
        }
        if let Some(line) = new_line {
            out.push_str(&format!("\nIt is now at line {line}."));
        }
        let diff = a_diff(old, new);
        if !diff.is_empty() {
            out.push_str("\n\n");
            out.push_str(&diff);
        }
        Ok(out)
    }
}

/// The line a text starts on, counted from one.
fn line_of(text: &str, needle: &str) -> Option<usize> {
    let position = text.find(needle)?;
    // The lines before the match, plus the line it is on.
    Some(text[..position].lines().count() + 1)
}

/// The difference between the old text and the new one.
fn a_diff(old: &str, new: &str) -> String {
    let mut out = String::new();
    for line in old.lines() {
        out.push_str(&format!("-{line}\n"));
    }
    for line in new.lines() {
        out.push_str(&format!("+{line}\n"));
    }
    out.trim_end().to_string()
}

/// Resolve a path against the project root.
fn resolve(root: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    }
}

/// The schema of the `read` tool.
pub fn read_schema() -> ToolSchema {
    ToolSchema::new(
        "read",
        "Read a file. The lines carry the anchors: the first line and every tenth line start \
         with the line number and a →. The anchor is not part of the file.",
        r#"{"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["path"]}"#,
        Risk::Read,
    )
}

/// The schema of the `write` tool.
pub fn write_schema() -> ToolSchema {
    ToolSchema::new(
        "write",
        "Create a file, or replace its content. The folders above it are created when they are \
         missing.",
        r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
        Risk::Write,
    )
}

/// The schema of the `edit` tool.
pub fn edit_schema() -> ToolSchema {
    ToolSchema::new(
        "edit",
        "Replace an exact string in a file. The text to replace must match exactly one place: \
         add the lines around it to make it unique, or set replace_all to change every \
         occurrence. An empty text to replace creates the file. The anchors of the read (the \
         line number and a →) are not part of the file.",
        r#"{"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["path","old_string","new_string"]}"#,
        Risk::Write,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-files-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is created");
        dir
    }

    #[test]
    fn the_read_numbers_the_lines() {
        let dir = a_dir("read");
        let mut text = String::new();
        for number in 1..=25 {
            text.push_str(&format!("line {number}\n"));
        }
        std::fs::write(dir.join("a.txt"), &text).expect("written");

        let out = ReadTool::new(dir.clone())
            .read("a.txt", None, None)
            .expect("read");
        let lines: Vec<&str> = out.lines().collect();
        // The first line and every tenth line carry the anchor.
        assert!(lines[0].starts_with("1→"), "{}", lines[0]);
        assert!(lines[1].starts_with("line 2"), "{}", lines[1]);
        assert!(lines[9].starts_with("10→"), "{}", lines[9]);
        assert!(lines[19].starts_with("20→"), "{}", lines[19]);
        assert!(!lines[2].starts_with("3→"), "{}", lines[2]);
    }

    #[test]
    fn the_edit_replaces_and_says_where() {
        let dir = a_dir("edit");
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").expect("written");

        let out = EditTool::new(dir.clone())
            .edit("a.txt", "two", "TWO", false)
            .expect("edited");
        assert!(out.contains("has been updated"), "{out}");
        assert!(out.contains("line 2"), "{out}");
        assert!(out.contains("-two"), "{out}");
        assert!(out.contains("+TWO"), "{out}");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).expect("read"),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn a_text_that_is_not_unique_is_refused() {
        let dir = a_dir("unique");
        std::fs::write(dir.join("a.txt"), "x\nx\n").expect("written");
        let error = EditTool::new(dir.clone())
            .edit("a.txt", "x", "y", false)
            .expect_err("refused");
        assert!(error.to_string().contains("not unique"), "{error}");
    }

    #[test]
    fn every_occurrence_can_be_replaced() {
        let dir = a_dir("all");
        std::fs::write(dir.join("a.txt"), "x\nx\n").expect("written");
        EditTool::new(dir.clone())
            .edit("a.txt", "x", "y", true)
            .expect("edited");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.txt")).expect("read"),
            "y\ny\n"
        );
    }

    #[test]
    fn an_empty_text_creates_the_file() {
        let dir = a_dir("create");
        let out = EditTool::new(dir.clone())
            .edit("new.txt", "", "hello\n", false)
            .expect("created");
        assert!(out.contains("has been created"), "{out}");
        assert_eq!(
            std::fs::read_to_string(dir.join("new.txt")).expect("read"),
            "hello\n"
        );
    }

    #[test]
    fn the_write_creates_the_folders() {
        let dir = a_dir("write");
        let out = WriteTool::new(dir.clone())
            .write("deep/down/a.txt", "hello")
            .expect("written");
        assert!(out.contains("has been created"), "{out}");
        assert!(dir.join("deep/down/a.txt").exists());
    }
}
