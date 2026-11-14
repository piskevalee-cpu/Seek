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
//! tells it to act. The guards here are the same idea:
//!
//! - [`ThinkingGuard`] fires when the reasoning repeats itself or grows past what a turn
//!   should need.
//! - [`CallGuard`] fires when the same call comes back with the same arguments.
//!
//! The interrupt is a user message with a `<system-interrupt>` tag: the system prompt tells
//! the model that such a message comes from SEEK itself.

use serde_json::Value;

/// The tag of the messages that come from SEEK and not from the user.
pub const INTERRUPT_TAG: &str = "<system-interrupt";

/// How many characters of reasoning are too many for one turn.
pub const MAX_THINKING: usize = 1_000;

/// The longest block that counts as a short cycle.
pub const CYCLE_SHORT_MAX: usize = 60;

/// How many characters a short cycle has to cover.
pub const CYCLE_SHORT_MIN: usize = 180;

/// How many times a short block has to repeat.
pub const CYCLE_SHORT_REPEATS: usize = 4;

/// How many characters a long cycle has to cover.
pub const CYCLE_LONG_MIN: usize = 1_024;

/// How many times a long block has to repeat.
pub const CYCLE_LONG_REPEATS: usize = 3;

/// How many new characters there have to be before the cycle is looked for again.
pub const CYCLE_STRIDE: usize = 128;

/// How many times the same call has to come back to count as a loop.
pub const MAX_REPEATS: usize = 5;

/// The tools that are allowed to repeat: waiting for a task is the point of it.
pub const EXEMPT_TOOLS: [&str; 1] = ["wait"];

/// Watches the reasoning of a turn.
#[derive(Debug, Clone)]
pub struct ThinkingGuard {
    /// The reasoning so far.
    text: String,
    /// The model wrote an answer: it is not stalling.
    answered: bool,
    /// The guard fired already in this request.
    fired: bool,
    /// How many interrupts this turn may still send.
    budget: usize,
    /// How many characters there were at the last look for a cycle.
    looked_at: usize,
}

impl Default for ThinkingGuard {
    fn default() -> Self {
        ThinkingGuard {
            text: String::new(),
            answered: false,
            fired: false,
            budget: 2,
            looked_at: 0,
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

    /// The model wrote something the user sees: it answered, so it is not stalling.
    pub fn answered(&mut self) {
        self.answered = true;
    }

    /// Whether the reasoning repeats itself.
    ///
    /// The rule of the oh-my-pi harness (MIT): a short block that comes back four times
    /// covering at least [`CYCLE_SHORT_MIN`] characters, or a long block that comes back three
    /// times covering at least [`CYCLE_LONG_MIN`].
    fn in_a_cycle(&self) -> bool {
        let tail = &self.text[self.text.len().saturating_sub(4 * CYCLE_STRIDE * 32)..];
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
    /// The guard fires once per turn: the interrupt is sent once, and the model is given the
    /// chance to act on it.
    pub fn stall(&mut self) -> Option<Stall> {
        if self.answered || self.fired || self.budget == 0 {
            return None;
        }
        let stall = if self.look_now() {
            Stall::Cycle
        } else if self.text.chars().count() > MAX_THINKING {
            Stall::TooLong
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
    /// The reasoning repeats itself.
    Cycle,
    /// The reasoning grew past what a turn should need.
    TooLong,
}

impl Stall {
    /// The number the interrupt names.
    pub fn count(&self, guard: &ThinkingGuard) -> usize {
        match self {
            Stall::Cycle => CYCLE_SHORT_REPEATS,
            Stall::TooLong => guard.len(),
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
        Stall::TooLong => format!(
            "Reasoning interrupted: {count} characters of planning, and no tool was called."
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
    fn a_long_reasoning_without_a_tool_is_a_stall() {
        let mut guard = ThinkingGuard::default();
        // A reasoning that goes on without repeating itself.
        let mut text = String::new();
        for index in 0..200 {
            text.push_str(&format!(
                "the plan for the step {index} is to look at the file. "
            ));
        }
        guard.push(&text);
        assert_eq!(guard.stall(), Some(Stall::TooLong));
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
    fn an_answer_latches_the_guard_off() {
        let mut guard = ThinkingGuard::default();
        guard.push(&"thinking ".repeat(200));
        guard.answered();
        assert!(guard.stall().is_none());
    }

    #[test]
    fn the_guard_fires_once_per_turn() {
        let mut guard = ThinkingGuard::default();
        guard.push(&"thinking ".repeat(200));
        assert!(guard.stall().is_some());
        assert!(guard.stall().is_none());
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
}
