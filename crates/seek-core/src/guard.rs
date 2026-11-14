//! The guards of the agent loop (SPEC 9.1).
//!
//! A small model has two bad habits, and both of them look like the model is working:
//!
//! - **It thinks and never acts.** The reasoning goes on and on, and no tool is called. The
//!   turn ends with nothing done.
//! - **It repeats itself.** The same call with the same arguments, again and again, and the
//!   turn never ends.
//!
//! The oh-my-pi harness (MIT) watches for both and interrupts the model with a message that
//! tells it to act. The guards here are the same idea, with the same numbers:
//!
//! - [`ThinkingGuard`] fires when the reasoning repeats itself — an exact cycle, a run of
//!   near-identical paragraphs, or a run of paragraphs that recycle the wording and name
//!   nothing new.
//! - [`CallGuard`] fires when the same call comes back with the same arguments.
//!
//! The interrupt is a user message with a `<system-interrupt>` tag: the system prompt tells
//! the model that such a message comes from SEEK itself.

use serde_json::Value;

/// The tag of the messages that come from SEEK and not from the user.
pub const INTERRUPT_TAG: &str = "<system-interrupt";

/// How many times the same call has to come back to count as a loop.
pub const MAX_REPEATS: usize = 5;

/// The tools that are allowed to repeat: waiting for a task is the point of it.
pub const EXEMPT_TOOLS: [&str; 1] = ["wait"];

// The numbers of the oh-my-pi harness (MIT).

/// The longest block that counts as a short cycle.
const CYCLE_SHORT_MAX: usize = 60;
/// How many characters a short cycle has to cover.
const CYCLE_SHORT_MIN: usize = 180;
/// How many times a short block has to repeat.
const CYCLE_SHORT_REPEATS: usize = 4;
/// How many characters a long cycle has to cover.
const CYCLE_LONG_MIN: usize = 1_024;
/// How many times a long block has to repeat.
const CYCLE_LONG_REPEATS: usize = 3;
/// The tail of the text that is kept for the cycle.
const CYCLE_TAIL: usize = 4_096;
/// How many new characters there have to be before the cycle is looked for again.
const CYCLE_STRIDE: usize = 128;

/// A paragraph is at most this long.
const SEGMENT_CAP: usize = 700;
/// A paragraph has to be at least this long to count.
const SEGMENT_MIN: usize = 60;
/// How many paragraphs are kept for the comparison.
const SEGMENT_WINDOW: usize = 16;
/// How alike two paragraphs have to be.
const SEGMENT_SIMILARITY: f64 = 0.8;
/// How many paragraphs have to be seen before the heuristics count.
const SEGMENT_MIN_COUNT: usize = 8;
/// How many alike paragraphs in a row are a loop.
const SEGMENT_MIN_CLUSTER: usize = 4;
/// How many paragraphs of vocabulary are kept.
const LEX_WINDOW: usize = 8;
/// At or below this novelty a paragraph counts as recycled wording.
const LEX_NOVELTY_FLOOR: f64 = 0.2;
/// How many such paragraphs in a row are a loop.
const LEX_STALL_MIN_RUN: usize = 8;

/// Watches the reasoning of a turn.
#[derive(Debug, Clone)]
pub struct ThinkingGuard {
    /// The reasoning so far.
    text: String,
    /// The paragraphs that were not looked at yet.
    pending: String,
    /// The fingerprints of the last paragraphs.
    window: Vec<Vec<String>>,
    /// The words of the last paragraphs.
    words: Vec<Vec<String>>,
    /// The references of the last paragraphs.
    anchors: Vec<Vec<String>>,
    /// How many paragraphs in a row recycle the wording.
    stall: usize,
    /// How many paragraphs were seen.
    count: usize,
    /// The model wrote an answer: it is not stalling.
    answered: bool,
    /// The guard fired already in this request.
    fired: bool,
    /// How many interrupts this turn may still send.
    budget: usize,
    /// How many characters there were at the last look for a cycle.
    looked_at: usize,
    /// How many of the last paragraphs were alike.
    cluster: usize,
}

impl Default for ThinkingGuard {
    fn default() -> Self {
        ThinkingGuard {
            text: String::new(),
            pending: String::new(),
            window: Vec::new(),
            words: Vec::new(),
            anchors: Vec::new(),
            stall: 0,
            count: 0,
            answered: false,
            fired: false,
            budget: 2,
            looked_at: 0,
            cluster: 0,
        }
    }
}

impl ThinkingGuard {
    /// A guard with a given number of interrupts.
    pub fn with_budget(budget: usize) -> Self {
        ThinkingGuard {
            budget,
            ..ThinkingGuard::default()
        }
    }

    /// Feed the reasoning.
    pub fn push(&mut self, delta: &str) {
        self.text.push_str(delta);
        self.pending.push_str(delta);
        self.drain();
    }

    /// The paragraphs that are ready are looked at.
    fn drain(&mut self) {
        loop {
            // A paragraph ends at a blank line.
            let boundary = find_blank_line(&self.pending);
            let raw = match boundary {
                Some(at) => {
                    let raw = self.pending[..at].to_string();
                    self.pending = self.pending[at..]
                        .trim_start_matches(['\n', '\r', ' ', '\t'])
                        .to_string();
                    raw
                }
                // A paragraph that runs on is cut at the cap.
                None if self.pending.len() > SEGMENT_CAP => {
                    let raw = self.pending[..SEGMENT_CAP].to_string();
                    self.pending = self.pending[SEGMENT_CAP..].to_string();
                    raw
                }
                None => return,
            };
            for chunk in raw.as_bytes().chunks(SEGMENT_CAP) {
                let chunk = String::from_utf8_lossy(chunk).to_string();
                self.consume(&chunk);
            }
        }
    }

    /// Look at one paragraph.
    fn consume(&mut self, raw: &str) {
        let segment = strip_titles_and_code(raw);
        let normalized = normalize(&segment);
        if normalized.len() < SEGMENT_MIN {
            return;
        }

        // (a) Near-identical paragraphs: the same trigrams again and again.
        let fingerprint = shingles(&normalized);
        let mut cluster = 1;
        for previous in &self.window {
            if similarity(&fingerprint, previous) >= SEGMENT_SIMILARITY {
                cluster += 1;
            }
        }

        // (b) The stall: the wording is recycled and nothing new is named.
        let words: Vec<String> = normalized
            .split_whitespace()
            .map(str::to_string)
            .filter(|word| !word.is_empty())
            .collect();
        let mut known: Vec<String> = Vec::new();
        for set in &self.words {
            known.extend(set.iter().cloned());
        }
        let unseen = words.iter().filter(|word| !known.contains(word)).count();
        let novelty = if known.is_empty() {
            1.0
        } else {
            unseen as f64 / words.len() as f64
        };
        let anchors = references(&segment);
        let new_anchor = anchors
            .iter()
            .any(|anchor| !self.anchors.iter().any(|seen| seen.contains(anchor)));
        if novelty <= LEX_NOVELTY_FLOOR && !new_anchor {
            self.stall += 1;
        } else {
            self.stall = 0;
        }

        self.window.push(fingerprint);
        if self.window.len() > SEGMENT_WINDOW {
            self.window.remove(0);
        }
        self.words.push(words);
        if self.words.len() > LEX_WINDOW {
            self.words.remove(0);
        }
        self.anchors.push(anchors);
        if self.anchors.len() > LEX_WINDOW {
            self.anchors.remove(0);
        }
        self.count += 1;

        self.cluster = cluster;
    }

    /// The model wrote something the user sees: it answered, so it is not stalling.
    pub fn answered(&mut self) {
        self.answered = true;
    }

    /// How many characters there are since the last look for a cycle.
    fn since_the_last_look(&self) -> usize {
        self.text.len().saturating_sub(self.looked_at)
    }

    /// The cycle is looked for now and then, not on every character (SPEC 9.1).
    fn look_now(&mut self) -> bool {
        if self.since_the_last_look() < CYCLE_STRIDE {
            return false;
        }
        self.looked_at = self.text.len();
        self.in_a_cycle()
    }

    /// Whether the reasoning repeats itself word for word.
    ///
    /// The rule of the oh-my-pi harness (MIT): a short block that comes back four times
    /// covering at least [`CYCLE_SHORT_MIN`] characters, or a long block that comes back three
    /// times covering at least [`CYCLE_LONG_MIN`].
    fn in_a_cycle(&self) -> bool {
        let tail = &self.text[self.text.len().saturating_sub(CYCLE_TAIL)..];
        let n = tail.len();

        // A short block: four repeats.
        let short_max = CYCLE_SHORT_MAX.min(n / CYCLE_SHORT_REPEATS);
        for length in 1..=short_max {
            if length * CYCLE_SHORT_REPEATS < CYCLE_SHORT_MIN {
                continue;
            }
            if repeats_at(tail, length, CYCLE_SHORT_REPEATS) {
                return true;
            }
        }

        // A long block: three repeats.
        if n >= CYCLE_LONG_MIN {
            let long_min = (CYCLE_LONG_MIN / CYCLE_LONG_REPEATS).max(1);
            let long_max = (n / CYCLE_LONG_REPEATS).min(CYCLE_LONG_MIN);
            for length in long_min..=long_max {
                if repeats_at(tail, length, CYCLE_LONG_REPEATS) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether the model is stalling, and why.
    ///
    /// The guard fires once per request: the interrupt is sent once, and the model is given the
    /// chance to act on it.
    pub fn stall(&mut self) -> Option<Stall> {
        if self.answered || self.fired || self.budget == 0 {
            return None;
        }
        let stall = if self.look_now() {
            Stall::Cycle
        } else if self.count >= SEGMENT_MIN_COUNT {
            if self.cluster >= SEGMENT_MIN_CLUSTER {
                Stall::NearDuplicate
            } else if self.stall >= LEX_STALL_MIN_RUN {
                Stall::Stall
            } else {
                return None;
            }
        } else {
            return None;
        };
        self.fired = true;
        self.budget -= 1;
        Some(stall)
    }

    /// A new request of the turn: the guard may fire again, but the budget holds.
    pub fn new_request(&mut self) {
        self.fired = false;
        // The cycle is looked for again: the reasoning did not grow, but the model has had
        // another chance to act.
        self.looked_at = 0;
    }

    /// The number of characters of reasoning.
    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// Whether there is no reasoning at all.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Why the model is stalling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stall {
    /// The reasoning repeats itself word for word.
    Cycle,
    /// The reasoning says the same thing in other words.
    NearDuplicate,
    /// The reasoning recycles the wording and names nothing new.
    Stall,
}

impl Stall {
    /// The number the interrupt names.
    pub fn count(&self, guard: &ThinkingGuard) -> usize {
        match self {
            Stall::Cycle => CYCLE_SHORT_REPEATS,
            Stall::NearDuplicate => guard.count,
            Stall::Stall => guard.stall,
        }
    }
}

/// The message that tells the model to act.
pub fn thinking_interrupt(stall: Stall, count: usize) -> String {
    let reason = match stall {
        Stall::Cycle => format!(
            "Reasoning interrupted: the same block of text came back {CYCLE_SHORT_REPEATS} times in \
             a row, and no tool was called."
        ),
        Stall::NearDuplicate => format!(
            "Reasoning interrupted: {count} paragraphs said the same thing in other words, and no \
             tool was called."
        ),
        Stall::Stall => format!(
            "Reasoning interrupted: {count} paragraphs recycled the same wording and named nothing \
             new, and no tool was called."
        ),
    };
    format!(
        "<system-interrupt reason=\"reasoning_without_tool_calls\">\n{reason} Thinking alone \
         changes nothing: zero progress this turn; no tool ran.\n\nAct now, not further planning:\n- \
         Emit a real call to an available tool in the tool call format. Do not describe the call in \
         prose or reasoning - issue it.\n- Pick the smallest concrete next step and call the tool \
         that does it.\n</system-interrupt>"
    )
}

/// Watches the calls of a turn for a loop.
#[derive(Debug, Clone, Default)]
pub struct CallGuard {
    /// The hash of the last call.
    last: Option<u64>,
    /// How many times it came back.
    count: usize,
    /// The name of the last call.
    name: String,
    /// The arguments of the last call.
    arguments: String,
    /// The result of the last call.
    result: String,
    /// The guard fired already in this turn.
    fired: bool,
}

impl CallGuard {
    /// Record a call and its result.
    pub fn record(&mut self, name: &str, input: &Value, result: &str) {
        let hash = hash_of(name, input);
        if self.last == Some(hash) {
            self.count += 1;
        } else {
            self.last = Some(hash);
            self.count = 1;
            self.name = name.to_string();
            self.arguments = summarize(input);
        }
        self.result = summarize_text(result, 200);
    }

    /// Whether the model is calling the same thing over and over.
    pub fn looped(&mut self) -> Option<Repeat> {
        if self.fired || self.count < MAX_REPEATS {
            return None;
        }
        if EXEMPT_TOOLS.contains(&self.name.as_str()) {
            return None;
        }
        self.fired = true;
        Some(Repeat {
            name: self.name.clone(),
            count: self.count,
            arguments: self.arguments.clone(),
            result: self.result.clone(),
        })
    }
}

/// A call that came back too many times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeat {
    pub name: String,
    pub count: usize,
    pub arguments: String,
    pub result: String,
}

/// The message that tells the model to stop repeating itself.
pub fn repeat_interrupt(repeat: &Repeat) -> String {
    format!(
        "<system-interrupt reason=\"tool_call_loop_detected\">\nYou called `{}` {} consecutive times \
         with identical arguments:\n`{}`\n\nLast result (truncated): `{}`\n\nNEVER call `{}` with those \
         arguments again this turn. Use different arguments, choose another tool, or summarize the \
         findings and yield if the work is complete.\n</system-interrupt>",
        repeat.name, repeat.count, repeat.arguments, repeat.result, repeat.name
    )
}

/// The hash of a call: the tool and its arguments, with the keys in a fixed order.
///
/// The intent field is left out: the same call with a different intent is still the same call.
pub fn hash_of(name: &str, input: &Value) -> u64 {
    let canonical = canonicalize(input);
    let text = format!("{name}:{canonical}");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0001_0000_0000_01b3);
    }
    hash
}

/// The arguments of a call, with the keys in a fixed order.
fn canonicalize(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .iter()
                .filter(|key| key.as_str() != crate::prompt::INTENT_FIELD)
                .map(|key| format!("{key}:{}", canonicalize(&map[*key])))
                .collect();
            parts.join(",")
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonicalize).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

/// The arguments of a call, as a short text.
fn summarize(input: &Value) -> String {
    summarize_text(&canonicalize(input), 400)
}

/// A text, cut to a length.
fn summarize_text(text: &str, limit: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<&str>>().join(" ");
    if flat.chars().count() > limit {
        let mut out: String = flat.chars().take(limit).collect();
        out.push('\u{2026}');
        out
    } else {
        flat
    }
}

/// Whether the text ends with the same block, `repeats` times in a row.
fn repeats_at(text: &str, length: usize, repeats: usize) -> bool {
    let n = text.len();
    if length == 0 || n < length * repeats {
        return false;
    }
    let block = &text[n - length..n];
    (1..repeats).all(|index| {
        let at = n - index * length;
        &text[at - length..at] == block
    })
}

/// Where the next blank line is: the end of a paragraph.
fn find_blank_line(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            let mut next = index + 1;
            let mut spaces = 0;
            while next < bytes.len() && (bytes[next] == b' ' || bytes[next] == b'\t') {
                next += 1;
                spaces += 1;
            }
            if next < bytes.len() && bytes[next] == b'\n' && spaces == 0 {
                return Some(index);
            }
            index = next;
            continue;
        }
        index += 1;
    }
    None
}

/// Take the titles and the code lines off a paragraph.
///
/// A reasoning summarizer titles its thoughts (`**Momentum**`, `## Heading`); they change with
/// every thought, and they would hide a loop.
fn strip_titles_and_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        // A title: a heading, or a whole line of bold.
        let is_title = (trimmed.starts_with('#') && trimmed[1..].starts_with([' ', '\t']))
            || (trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4);
        // A line of code: it is indented, or it ends in a bracket.
        let is_code = line.starts_with("  ")
            || line.starts_with('\t')
            || trimmed.ends_with(['{', '}', '[', ']', '(', ')', ';', ',', '>']);
        if is_title || is_code {
            continue;
        }
        out.push_str(line);
        out.push(' ');
    }
    out
}

/// A paragraph, in a form that can be compared: lower case, no numbers, no punctuation.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        let word: String = word
            .chars()
            .filter(|character| character.is_alphabetic())
            .collect::<String>()
            .to_lowercase();
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word);
    }
    out
}

/// The trigrams of a paragraph: the fingerprint two paragraphs are compared with.
fn shingles(normalized: &str) -> Vec<String> {
    let words: Vec<&str> = normalized.split_whitespace().collect();
    if words.len() < 3 {
        return words.iter().map(|word| word.to_string()).collect();
    }
    (0..=words.len() - 3)
        .map(|index| format!("{} {} {}", words[index], words[index + 1], words[index + 2]))
        .collect()
}

/// How alike two paragraphs are.
fn similarity(left: &[String], right: &[String]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let mut shared = 0;
    for shingle in left {
        if right.contains(shingle) {
            shared += 1;
        }
    }
    let union = left.len() + right.len() - shared;
    if union == 0 {
        0.0
    } else {
        shared as f64 / union as f64
    }
}

/// The concrete references a paragraph names.
///
/// A path, an identifier, a code span: something the model is really working on. A paragraph
/// that names nothing new is filler.
fn references(segment: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes: Vec<char> = segment.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        // A code span.
        if bytes[index] == '`' {
            let mut end = index + 1;
            while end < bytes.len() && bytes[end] != '`' {
                end += 1;
            }
            if end < bytes.len() && end > index + 1 {
                let span: String = bytes[index + 1..end].iter().collect();
                push_reference(&mut out, &span);
            }
            index = end + 1;
            continue;
        }
        // A word: a path, a dotted name, or an identifier.
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_alphanumeric()
                || bytes[index] == '_'
                || bytes[index] == '-'
                || bytes[index] == '.'
                || bytes[index] == '/')
        {
            index += 1;
        }
        if index > start {
            let word: String = bytes[start..index].iter().collect();
            if word.contains('/') || word.contains('.') || word.contains('_') {
                push_reference(&mut out, &word);
            }
        }
        if index == start {
            index += 1;
        }
    }
    out
}

/// A reference, in a form that says the same thing twice.
fn push_reference(out: &mut Vec<String>, word: &str) {
    let word = word.trim_matches(|c: char| {
        !c.is_alphanumeric() && c != '_' && c != '-' && c != '.' && c != '/'
    });
    if word.len() < 2 {
        return;
    }
    let canonical = word.to_lowercase();
    if !out.contains(&canonical) {
        out.push(canonical);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_short_reasoning_is_not_a_stall() {
        let mut guard = ThinkingGuard::default();
        guard.push("Let me look at the file. ");
        assert!(guard.stall().is_none());
    }

    #[test]
    fn a_repeated_block_is_a_stall() {
        let mut guard = ThinkingGuard::default();
        let block = "the user wants to know about the sea and the tide. ".repeat(2);
        guard.push(&block);
        guard.push(&block);
        assert_eq!(guard.stall(), Some(Stall::Cycle));
    }

    #[test]
    fn the_same_paragraph_in_other_words_is_a_stall() {
        let mut guard = ThinkingGuard::default();
        // Four paragraphs that say the same thing with a different wording.
        for index in 0..9 {
            guard.push(&format!(
                "We should look at the file and then decide what to do with the number inside it, \
                 step {index}.\n\n"
            ));
        }
        let stall = guard.stall();
        assert!(
            matches!(stall, Some(Stall::NearDuplicate) | Some(Stall::Stall)),
            "{stall:?}"
        );
    }

    #[test]
    fn an_answer_latches_the_guard_off() {
        let mut guard = ThinkingGuard::default();
        guard.push(&"thinking ".repeat(200));
        guard.answered();
        assert!(guard.stall().is_none());
    }

    #[test]
    fn the_guard_fires_once_per_request() {
        let mut guard = ThinkingGuard::default();
        let block = "the user wants to know about the sea and the tide. ".repeat(2);
        guard.push(&block);
        guard.push(&block);
        assert!(guard.stall().is_some());
        assert!(guard.stall().is_none());
        // A new request: the guard may fire again.
        guard.new_request();
        assert!(guard.stall().is_some());
    }

    #[test]
    fn the_same_call_again_and_again_is_a_loop() {
        let mut guard = CallGuard::default();
        for _ in 0..4 {
            guard.record("write", &json!({"path": "a.txt"}), "wrote");
        }
        assert!(guard.looped().is_none());
        guard.record("write", &json!({"path": "a.txt"}), "wrote");
        let repeat = guard.looped().expect("a loop");
        assert_eq!(repeat.count, 5);
        assert_eq!(repeat.name, "write");
    }

    #[test]
    fn a_different_call_is_not_a_loop() {
        let mut guard = CallGuard::default();
        for index in 0..10 {
            guard.record("read", &json!({"path": format!("{index}.txt")}), "read");
        }
        assert!(guard.looped().is_none());
    }

    #[test]
    fn the_intent_is_not_part_of_the_call() {
        // The same call with another intent is still the same call.
        let with = |intent: &str| json!({"path": "a.txt", "i": intent});
        assert_eq!(
            hash_of("write", &with("one")),
            hash_of("write", &with("two"))
        );
    }

    #[test]
    fn the_interrupt_names_the_tool() {
        let repeat = Repeat {
            name: "write".into(),
            count: 5,
            arguments: "path:a.txt".into(),
            result: "wrote".into(),
        };
        let message = repeat_interrupt(&repeat);
        assert!(message.contains("write"));
        assert!(message.contains("5 consecutive times"));
        assert!(message.starts_with(INTERRUPT_TAG));
    }

    #[test]
    fn the_thinking_interrupt_says_to_act() {
        let message = thinking_interrupt(Stall::Cycle, 4);
        assert!(message.contains("Act now"));
        assert!(message.contains("<system-interrupt"));
    }
}
