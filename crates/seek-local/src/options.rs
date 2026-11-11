//! The launch options of a local model (SPEC 7.2, 7.4).
//!
//! SEEK runs `llama serve -m <path> --host 127.0.0.1 --port <port>` followed by the launch
//! options of the user.
//!
//! The user never types `-m`, `--host` or `--port`: if they do, the option is stripped and the
//! user is warned. `--reasoning-format deepseek`, `--parallel 1` and `--jinja` are implicit,
//! unless the user overrides them.

use std::path::Path;

/// The options SEEK always sets, unless the user overrides them.
pub const IMPLICIT_MODEL: &str = "--model";
pub const IMPLICIT_HOST: &str = "--host";
pub const IMPLICIT_PORT: &str = "--port";
pub const IMPLICIT_REASONING_FORMAT: &str = "--reasoning-format";
pub const IMPLICIT_PARALLEL: &str = "--parallel";
pub const IMPLICIT_JINJA: &str = "--jinja";
pub const IMPLICIT_CONTEXT: &str = "--ctx-size";

/// The short forms of the options SEEK owns.
///
/// The user may write `-c 8000` instead of `--ctx-size 8000`; both are the same option, and
/// SEEK has to see that the user set it.
const SHORT_FORMS: [(&str, &str); 3] = [
    (IMPLICIT_MODEL, "-m"),
    (IMPLICIT_CONTEXT, "-c"),
    (IMPLICIT_PARALLEL, "-np"),
];

/// The name and the short form of an option, as a pair of needles.
fn needles(name: &str) -> Vec<&str> {
    let mut out = vec![name];
    if let Some((_, short)) = SHORT_FORMS.iter().find(|(long, _)| *long == name) {
        out.push(short);
    }
    out
}
/// The context size SEEK asks for, unless the user sets one.
///
/// The context of a model can be huge (131072 tokens for a small model), and the key and value
/// cache of the whole context is allocated up front: 6.5 GB for that model. A coding agent does
/// not need that much, so SEEK asks for a sane one and the user can override it.
pub const DEFAULT_CONTEXT: u32 = 32_768;

/// The options of the user, with the ones SEEK owns removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// The options of the user, split into words.
    pub words: Vec<String>,
    /// What was stripped, so the user can be warned.
    pub stripped: Vec<String>,
}

impl LaunchOptions {
    /// Parse the free text of the editor.
    pub fn parse(text: &str) -> Self {
        let words = shell_words::split(text).unwrap_or_else(|_| {
            text.split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        });

        let mut kept = Vec::new();
        let mut stripped = Vec::new();
        let mut index = 0;
        while index < words.len() {
            let word = &words[index];
            let (name, takes_value) = match word.split_once('=') {
                Some((name, _)) => (name, true),
                None => (word.as_str(), !is_flag(word)),
            };
            // `-m` is the short form of `--model` and is stripped as well.
            let owned = name == "-m";
            let name = if owned { IMPLICIT_MODEL } else { name };
            match name {
                IMPLICIT_MODEL | IMPLICIT_HOST | IMPLICIT_PORT => {
                    stripped.push(word.clone());
                    if !takes_value {
                        // The value is the next word.
                        if index + 1 < words.len() {
                            stripped.push(words[index + 1].clone());
                            index += 1;
                        }
                    }
                }
                _ => {
                    kept.push(word.clone());
                    if !takes_value && index + 1 < words.len() && !is_flag(&words[index + 1]) {
                        kept.push(words[index + 1].clone());
                        index += 1;
                    }
                }
            }
            index += 1;
        }

        LaunchOptions {
            words: kept,
            stripped,
        }
    }

    /// Whether the user set an option themselves.
    pub fn has(&self, name: &str) -> bool {
        let needles = needles(name);
        self.words.iter().any(|word| {
            needles
                .iter()
                .any(|needle| word == needle || word.starts_with(&format!("{needle}=")))
        })
    }

    /// The options that SEEK adds because the user did not set them.
    pub fn implicit(&self) -> Vec<String> {
        let mut implicit = Vec::new();
        if !self.has(IMPLICIT_REASONING_FORMAT) {
            implicit.push(IMPLICIT_REASONING_FORMAT.to_string());
            implicit.push("deepseek".to_string());
        }
        if !self.has(IMPLICIT_PARALLEL) {
            implicit.push(IMPLICIT_PARALLEL.to_string());
            implicit.push("1".to_string());
        }
        if !self.has(IMPLICIT_JINJA) {
            implicit.push(IMPLICIT_JINJA.to_string());
        }
        if !self.has(IMPLICIT_CONTEXT) {
            implicit.push(IMPLICIT_CONTEXT.to_string());
            implicit.push(DEFAULT_CONTEXT.to_string());
        }
        implicit
    }

    /// The whole command, as it is run.
    ///
    /// The `subcommand` is the one that starts the server: `serve` for the unified `llama`
    /// binary, nothing for the legacy `llama-server` (DECISIONS.md D-001).
    pub fn command(
        &self,
        binary: &str,
        subcommand: Option<&str>,
        model: &Path,
        port: u16,
    ) -> Vec<String> {
        let mut command = vec![binary.to_string()];
        if let Some(subcommand) = subcommand {
            command.push(subcommand.to_string());
        }
        command.extend([
            IMPLICIT_MODEL.to_string(),
            model.display().to_string(),
            IMPLICIT_HOST.to_string(),
            "127.0.0.1".to_string(),
            IMPLICIT_PORT.to_string(),
            port.to_string(),
        ]);
        command.extend(self.words.iter().cloned());
        command.extend(self.implicit());
        command
    }

    /// The read-only prefix the editor shows.
    pub fn prefix(&self, subcommand: Option<&str>, model: &Path, port: u16) -> String {
        let mut prefix = String::from("llama");
        if let Some(subcommand) = subcommand {
            prefix.push(' ');
            prefix.push_str(subcommand);
        }
        prefix.push_str(&format!(
            " -m {} --host 127.0.0.1 --port {port}",
            model.display()
        ));
        prefix
    }
}

/// Whether a word is an option and not a value.
fn is_flag(word: &str) -> bool {
    word.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> std::path::PathBuf {
        std::path::PathBuf::from("/models/mistral.gguf")
    }

    #[test]
    fn the_command_has_the_implicit_options() {
        let options = LaunchOptions::parse("");
        let command = options.command("llama", Some("serve"), &model(), 8123);
        assert_eq!(command[0], "llama");
        assert_eq!(command[1], "serve");
        assert!(command.contains(&"--model".to_string()));
        assert!(command.contains(&"/models/mistral.gguf".to_string()));
        assert!(command.contains(&"127.0.0.1".to_string()));
        assert!(command.contains(&"8123".to_string()));
        assert!(command.contains(&"deepseek".to_string()));
        assert!(command.contains(&"--parallel".to_string()));
        assert!(command.contains(&"--jinja".to_string()));
    }

    #[test]
    fn the_model_the_host_and_the_port_are_stripped() {
        let options = LaunchOptions::parse("--model other.gguf --host 0.0.0.0 --port 1234");
        assert_eq!(options.stripped.len(), 6);
        assert!(options.words.is_empty());
    }

    #[test]
    fn the_short_form_of_the_context_size_counts() {
        // `-c 8000` is `--ctx-size 8000`: SEEK must not add its own.
        let options = LaunchOptions::parse("-c 8000");
        assert!(options.has(IMPLICIT_CONTEXT));
        let command = options.command("llama", Some("serve"), &model(), 8123);
        assert_eq!(command.iter().filter(|word| *word == "8000").count(), 1);
        assert!(!command.contains(&"32768".to_string()), "{command:?}");
    }

    #[test]
    fn the_short_form_of_the_model_is_stripped() {
        let options = LaunchOptions::parse("-m other.gguf");
        assert!(options.words.is_empty(), "{:?}", options.words);
        assert_eq!(options.stripped.len(), 2);
    }

    #[test]
    fn the_context_size_is_implicit_but_can_be_overridden() {
        let options = LaunchOptions::parse("");
        let command = options.command("llama", Some("serve"), &model(), 8123);
        assert!(command.contains(&"32768".to_string()), "{command:?}");

        let options = LaunchOptions::parse("--ctx-size 8192");
        let command = options.command("llama", Some("serve"), &model(), 8123);
        assert_eq!(command.iter().filter(|word| *word == "8192").count(), 1);
        assert!(!command.contains(&"32768".to_string()));
    }

    #[test]
    fn the_options_with_an_equal_sign_are_stripped_too() {
        let options = LaunchOptions::parse("--model=other.gguf --port=1234");
        assert!(options.words.is_empty());
        assert_eq!(options.stripped.len(), 2);
    }

    #[test]
    fn the_user_may_override_an_implicit_option() {
        let options = LaunchOptions::parse("--parallel 4 --reasoning-format none");
        let command = options.command("llama", Some("serve"), &model(), 8123);
        assert_eq!(command.iter().filter(|word| *word == "4").count(), 1);
        assert!(!command.contains(&"deepseek".to_string()));
        assert!(command.contains(&"none".to_string()));
    }

    #[test]
    fn the_options_of_the_user_are_kept() {
        let options = LaunchOptions::parse("--ctx-size 8192 -ngl 99");
        assert_eq!(options.words, vec!["--ctx-size", "8192", "-ngl", "99"]);
        assert!(options.stripped.is_empty());
    }

    #[test]
    fn the_prefix_is_read_only() {
        let options = LaunchOptions::parse("--ctx-size 8192");
        assert_eq!(
            options.prefix(Some("serve"), &model(), 8123),
            "llama serve -m /models/mistral.gguf --host 127.0.0.1 --port 8123"
        );
        // The legacy binary is the server itself.
        assert_eq!(
            options.prefix(None, &model(), 8123),
            "llama -m /models/mistral.gguf --host 127.0.0.1 --port 8123"
        );
    }

    #[test]
    fn a_quoted_option_stays_together() {
        let options = LaunchOptions::parse("--chat-template-kwargs '{\"enable_thinking\": true}'");
        assert_eq!(options.words.len(), 2);
        assert!(options.words[1].contains("enable_thinking"));
    }
}
