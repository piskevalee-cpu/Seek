//! The multi-line prompt input (SPEC 10.3).
//!
//! Hand-written on purpose: `tui-textarea` depends on ratatui 0.29 and cannot be used with
//! ratatui 0.30 (see `docs/DECISIONS.md`).
//!
//! Enter sends, Shift+Enter and Alt+Enter insert a newline, Up and Down walk the history on a
//! single-line draft.

/// The slash commands the prompt bar offers.
pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/compact", "Summarize the older turns"),
    ("/help", "Show the key reference"),
    ("/mode", "Set the permission mode"),
    ("/model", "Pick a model"),
    ("/new", "Start a new session"),
    ("/provider", "Add or edit providers"),
    ("/quit", "Exit SEEK"),
    ("/resume", "Open the sessions"),
    ("/settings", "Your name and settings"),
    ("/skills", "Toggle the skills"),
    ("/think", "Set the reasoning level"),
    ("/thoughts", "Show or hide reasoning text"),
    ("/tools", "Toggle the tools"),
    ("/truncate", "Drop the oldest turns"),
];

/// A multi-line text input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    lines: Vec<String>,
    row: usize,
    column: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    draft: Option<String>,
}

impl Input {
    pub fn new() -> Self {
        Input {
            lines: vec![String::new()],
            ..Input::default()
        }
    }

    /// The whole text, lines joined with newlines.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Replace the text.
    pub fn set_text(&mut self, text: &str) {
        self.lines = text.split('\n').map(str::to_string).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.lines.len() - 1;
        self.column = char_len(&self.lines[self.row]);
    }

    /// Whether there is nothing to send.
    pub fn is_empty(&self) -> bool {
        self.text().trim().is_empty()
    }

    /// The current line.
    pub fn current_line(&self) -> &str {
        self.lines.get(self.row).map(String::as_str).unwrap_or("")
    }

    /// The cursor position as (row, column in characters).
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.column)
    }

    /// The number of lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the input has no lines (never true; kept for `clippy::len_without_is_empty`).
    pub fn is_empty_lines(&self) -> bool {
        self.lines.is_empty()
    }

    /// Insert a character at the cursor.
    pub fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.row];
        let index = byte_index(line, self.column);
        line.insert(index, c);
        self.column += 1;
    }

    /// Insert a newline at the cursor.
    pub fn insert_newline(&mut self) {
        let line = self.lines[self.row].clone();
        let index = byte_index(&line, self.column);
        let head = line[..index].to_string();
        let tail = line[index..].to_string();
        self.lines[self.row] = head;
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.column = 0;
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.column > 0 {
            let line = &mut self.lines[self.row];
            let index = byte_index(line, self.column - 1);
            line.remove(index);
            self.column -= 1;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            self.column = char_len(&self.lines[self.row]);
            self.lines[self.row].push_str(&current);
        }
    }

    /// Delete the character under the cursor.
    pub fn delete(&mut self) {
        let line = &mut self.lines[self.row];
        if self.column < char_len(line) {
            let index = byte_index(line, self.column);
            line.remove(index);
        }
    }

    /// Move the cursor one character left.
    pub fn move_left(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.column = char_len(&self.lines[self.row]);
        }
    }

    /// Move the cursor one character right.
    pub fn move_right(&mut self) -> bool {
        if self.column < char_len(&self.lines[self.row]) {
            self.column += 1;
            true
        } else {
            self.row + 1 < self.lines.len()
        }
    }

    /// Move the cursor one line up.
    pub fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.column = self.column.min(char_len(&self.lines[self.row]));
        }
    }

    /// Move the cursor one line down.
    pub fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.column = self.column.min(char_len(&self.lines[self.row]));
        }
    }

    /// Move to the start of the line.
    pub fn move_home(&mut self) {
        self.column = 0;
    }

    /// Move to the end of the line.
    pub fn move_end(&mut self) {
        self.column = char_len(&self.lines[self.row]);
    }

    /// Take the text, clear the input and remember it in the history.
    pub fn submit(&mut self) -> Option<String> {
        let text = self.text();
        if text.trim().is_empty() {
            return None;
        }
        self.lines = vec![String::new()];
        self.row = 0;
        self.column = 0;
        self.history_index = None;
        self.draft = None;
        self.history.push(text.clone());
        Some(text)
    }

    /// Walk back through the history. Only on a single-line draft.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() || self.lines.len() > 1 {
            return;
        }
        let index = match self.history_index {
            None => {
                self.draft = Some(self.text());
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.history_index = Some(index);
        let entry = self.history[index].clone();
        self.lines = vec![entry];
        self.row = 0;
        self.column = char_len(&self.lines[0]);
    }

    /// Walk forward through the history.
    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            let draft = self.draft.take().unwrap_or_default();
            self.lines = vec![draft];
        } else {
            self.history_index = Some(index + 1);
            self.lines = vec![self.history[index + 1].clone()];
        }
        self.row = 0;
        self.column = char_len(&self.lines[0]);
    }

    /// The slash command suggestions for the current line.
    pub fn suggestions(&self) -> Vec<&'static str> {
        let line = self.current_line();
        if !line.starts_with('/') || line.contains(' ') {
            return Vec::new();
        }
        SLASH_COMMANDS
            .iter()
            .map(|(command, _)| *command)
            .filter(|command| command.starts_with(line))
            .collect()
    }

    /// Whether the current line is exactly one of the slash commands.
    pub fn slash_command(&self) -> Option<&'static str> {
        let line = self.current_line();
        SLASH_COMMANDS
            .iter()
            .map(|(command, _)| *command)
            .find(|command| *command == line)
    }
}

/// The number of characters in a string.
fn char_len(text: &str) -> usize {
    text.chars().count()
}

/// The byte index of the given character position.
fn byte_index(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_submitting() {
        let mut input = Input::new();
        for c in "hello".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.text(), "hello");
        assert_eq!(input.submit().as_deref(), Some("hello"));
        assert!(input.is_empty());
    }

    #[test]
    fn empty_input_is_not_submitted() {
        let mut input = Input::new();
        input.insert_char(' ');
        assert_eq!(input.submit(), None);
    }

    #[test]
    fn newline_splits_the_lines() {
        let mut input = Input::new();
        input.insert_char('a');
        input.insert_newline();
        input.insert_char('b');
        assert_eq!(input.text(), "a\nb");
        assert_eq!(input.cursor(), (1, 1));
    }

    #[test]
    fn backspace_joins_lines() {
        let mut input = Input::new();
        input.insert_char('a');
        input.insert_newline();
        input.insert_char('b');
        input.backspace();
        input.backspace();
        assert_eq!(input.text(), "a");
        assert_eq!(input.cursor(), (0, 1));
    }

    #[test]
    fn the_history_is_walked_and_restored() {
        let mut input = Input::new();
        input.insert_char('1');
        input.submit();
        input.insert_char('2');
        input.submit();
        input.insert_char('x');

        input.history_prev();
        assert_eq!(input.text(), "2");
        input.history_prev();
        assert_eq!(input.text(), "1");
        input.history_next();
        assert_eq!(input.text(), "2");
        input.history_next();
        assert_eq!(input.text(), "x");
    }

    #[test]
    fn suggestions_only_appear_for_a_slash() {
        let mut input = Input::new();
        assert!(input.suggestions().is_empty());
        for c in "/thoughts".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.suggestions(), vec!["/thoughts"]);
        assert_eq!(input.slash_command(), Some("/thoughts"));

        // A prefix that is not a command yet is not one.
        let mut input = Input::new();
        for c in "/th".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.slash_command(), None);

        // A shorter prefix offers every command that starts with it.
        let mut input = Input::new();
        for c in "/th".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.suggestions(), vec!["/think", "/thoughts"]);
    }

    #[test]
    fn a_known_slash_command_is_recognized() {
        let mut input = Input::new();
        for c in "/help".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.slash_command(), Some("/help"));
    }

    #[test]
    fn unicode_is_handled_by_character_not_byte() {
        let mut input = Input::new();
        for c in "äöü".chars() {
            input.insert_char(c);
        }
        assert_eq!(input.text(), "äöü");
        input.backspace();
        assert_eq!(input.text(), "äö");
    }
}
