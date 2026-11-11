//! The configuration files (SPEC 5, 6).
//!
//! `~/seek/providers.toml` holds the cloud providers and their models, `~/seek/config.toml` the
//! theme, the default mode, the default model and the path to the llama.cpp server.
//!
//! Every file carries `schema = 1`. Changing a format needs a migration (SPEC 2.4).
//!
//! API keys never live in these files: a provider names the environment variables its key is in
//! (SPEC 5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The schema version of the configuration files.
pub const SCHEMA: u32 = 1;

/// The SEEK home, `~/seek` by default.
///
/// `SEEK_HOME` overrides it, which is handy for tests.
pub fn seek_home() -> PathBuf {
    if let Some(home) = std::env::var_os("SEEK_HOME") {
        return PathBuf::from(home);
    }
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("seek")
}

/// The folder inside the SEEK home that holds the cloud providers.
pub const PROVIDERS_FILE: &str = "providers.toml";
/// The file inside the SEEK home that holds the settings.
pub const CONFIG_FILE: &str = "config.toml";

/// The kind of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// The Anthropic Messages API.
    AnthropicMessages,
    /// Any OpenAI-compatible server.
    OpenaiChat,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::AnthropicMessages => "anthropic-messages",
            ProviderKind::OpenaiChat => "openai-chat",
        }
    }

    /// The kind of a name, if it is one.
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "anthropic-messages" => Some(ProviderKind::AnthropicMessages),
            "openai-chat" => Some(ProviderKind::OpenaiChat),
            _ => None,
        }
    }
}

/// One model of a provider.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    /// The name shown in the picker.
    #[serde(default)]
    pub name: Option<String>,
    /// The context window in tokens.
    #[serde(default)]
    pub context: Option<u32>,
    /// Client-side requests per minute.
    #[serde(default)]
    pub rpm: Option<u32>,
    /// Client-side tokens per minute.
    #[serde(default)]
    pub tpm: Option<u32>,
    /// The variant used when the user has not chosen one.
    #[serde(default)]
    pub variant_default: Option<String>,
    /// The variants this model declares.
    #[serde(default)]
    pub variants: BTreeMap<String, serde_json::Value>,
    /// A disabled model is not offered.
    #[serde(default)]
    pub disabled: Option<bool>,
}

/// One provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// The name shown in the picker.
    #[serde(default)]
    pub name: Option<String>,
    /// The wire protocol.
    #[serde(default)]
    pub kind: Option<String>,
    /// The base URL, without a trailing slash.
    #[serde(default)]
    pub base_url: Option<String>,
    /// The environment variables the key is in.
    #[serde(default)]
    pub env: Vec<String>,
    /// The models of this provider.
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
}

impl ProviderConfig {
    /// The display name of the provider.
    pub fn display_name(&self, id: &str) -> String {
        self.name.clone().unwrap_or_else(|| id.to_string())
    }

    /// The kind of the provider, if it has a known one.
    pub fn provider_kind(&self) -> Option<ProviderKind> {
        self.kind.as_deref().and_then(ProviderKind::parse)
    }

    /// The key of the provider, read from the first environment variable that is set.
    ///
    /// The key is never stored in the file (SPEC 5).
    pub fn api_key(&self) -> Option<String> {
        for name in &self.env {
            if let Ok(value) = std::env::var(name)
                && !value.is_empty()
            {
                return Some(value);
            }
        }
        None
    }

    /// The key of the provider, from the environment or from the auth file.
    pub fn api_key_with(&self, auth: &Auth) -> Option<String> {
        if let Some(key) = self.api_key() {
            return Some(key);
        }
        self.env
            .iter()
            .find_map(|name| auth.key(name).map(str::to_string))
    }
}

/// The contents of `providers.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvidersConfig {
    /// The schema version; always 1 for now.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The providers, by id.
    #[serde(default)]
    pub provider: BTreeMap<String, ProviderConfig>,
}

fn default_schema() -> u32 {
    SCHEMA
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        ProvidersConfig {
            schema: SCHEMA,
            provider: BTreeMap::new(),
        }
    }
}

impl ProvidersConfig {
    /// Parse the file.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Read the file.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
        })
    }

    /// Write the file.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }

    /// The providers that are not empty.
    pub fn names(&self) -> Vec<&str> {
        self.provider.keys().map(String::as_str).collect()
    }

    /// The models of a provider, with their ids.
    pub fn models(&self, provider: &str) -> Vec<(&str, &ModelConfig)> {
        match self.provider.get(provider) {
            Some(config) => config
                .models
                .iter()
                .filter(|(_, model)| !model.disabled.unwrap_or(false))
                .map(|(id, model)| (id.as_str(), model))
                .collect(),
            None => Vec::new(),
        }
    }
}

/// The keys of the user, in `state/auth.json`.
///
/// The file is written with the mode 0600; the keys are never in `config.toml` or
/// `providers.toml` (SPEC 5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    /// The keys, by the name of the environment variable they stand in for.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
}

impl Auth {
    /// Read the file.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
        })
    }

    /// Write the file, readable by the owner only.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())?;
        restrict_to_the_owner(path)
    }

    /// The key with that name.
    pub fn key(&self, name: &str) -> Option<&str> {
        self.keys.get(name).map(String::as_str)
    }
}

/// Make a file readable and writable by its owner only.
fn restrict_to_the_owner(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// The contents of `config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// The schema version; always 1 for now.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The name of the theme.
    #[serde(default)]
    pub theme: Option<String>,
    /// The default permission mode.
    #[serde(default)]
    pub default_mode: Option<String>,
    /// The default model, as `provider/model`.
    #[serde(default)]
    pub default_model: Option<String>,
    /// The path to the llama.cpp binary.
    ///
    /// The shipped llama.cpp is one binary whose server is a subcommand, so the subcommand is a
    /// key of its own (DECISIONS.md D-001).
    #[serde(default)]
    pub llama_path: Option<String>,
    /// The subcommand that starts the server; `serve` for the unified `llama` binary, empty for
    /// the legacy `llama-server`.
    #[serde(default)]
    pub llama_subcommand: Option<String>,
    /// The old key, kept so that an old file still works. It is migrated away on the first write.
    #[serde(default, skip_serializing)]
    pub llama_server_path: Option<String>,
    /// Whether reasoning text is shown.
    #[serde(default)]
    pub show_reasoning: Option<bool>,
    /// Whether the animations are replaced by static text.
    #[serde(default)]
    pub reduce_motion: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema: SCHEMA,
            theme: None,
            default_mode: None,
            default_model: None,
            llama_path: None,
            llama_subcommand: None,
            llama_server_path: None,
            show_reasoning: None,
            reduce_motion: None,
        }
    }
}

impl Config {
    /// Parse the file.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Read the file.
    pub fn read(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
        })
    }

    /// Write the file.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }

    /// Move the old key to the new one.
    ///
    /// The old `llama_server_path` pointed at a binary that was the server itself; the new keys
    /// point at the binary and at the subcommand that starts the server (DECISIONS.md D-001).
    ///
    /// Returns whether the file has to be written again.
    pub fn migrate(&mut self) -> bool {
        let Some(old) = self.llama_server_path.take() else {
            return false;
        };
        if self.llama_path.is_none() {
            self.llama_path = Some(old);
            // The legacy binary is the server itself, so it has no subcommand.
            self.llama_subcommand.get_or_insert_with(String::new);
            return true;
        }
        true
    }

    /// The command that starts the server: the binary and the subcommand.
    pub fn llama_command(&self) -> (String, Option<String>) {
        let path = self
            .llama_path
            .clone()
            .unwrap_or_else(|| "llama".to_string());
        let subcommand = self
            .llama_subcommand
            .clone()
            .filter(|subcommand| !subcommand.is_empty());
        (path, subcommand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
schema = 1
[provider.anthropic]
name = 'Anthropic'
kind = 'anthropic-messages'
base_url = 'https://api.anthropic.com'
env = ['ANTHROPIC_API_KEY']
[provider.anthropic.models.'claude-sonnet-4-5']
name = 'Claude Sonnet 4.5'
context = 200000
rpm = 50
tpm = 100000
"#;

    #[test]
    fn the_example_from_the_spec_is_parsed() {
        let config = ProvidersConfig::parse(EXAMPLE).expect("the example parses");
        assert_eq!(config.schema, 1);
        let provider = &config.provider["anthropic"];
        assert_eq!(provider.name.as_deref(), Some("Anthropic"));
        assert_eq!(
            provider.provider_kind(),
            Some(ProviderKind::AnthropicMessages)
        );
        assert_eq!(provider.env, vec!["ANTHROPIC_API_KEY"]);
        let model = &provider.models["claude-sonnet-4-5"];
        assert_eq!(model.context, Some(200_000));
        assert_eq!(model.rpm, Some(50));
        assert_eq!(model.tpm, Some(100_000));
    }

    #[test]
    fn a_disabled_model_is_not_offered() {
        let mut config = ProvidersConfig::parse(EXAMPLE).expect("parses");
        config
            .provider
            .get_mut("anthropic")
            .unwrap()
            .models
            .get_mut("claude-sonnet-4-5")
            .unwrap()
            .disabled = Some(true);
        assert!(config.models("anthropic").is_empty());
    }

    #[test]
    fn the_keys_live_in_the_auth_file() {
        let dir = std::env::temp_dir().join("seek-auth-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state").join("auth.json");

        let mut auth = Auth::default();
        auth.keys
            .insert("ANTHROPIC_API_KEY".into(), "secret".into());
        auth.write(&path).expect("written");

        let read = Auth::read(&path).expect("read");
        assert_eq!(read.key("ANTHROPIC_API_KEY"), Some("secret"));

        // The file is readable by its owner only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the mode is {mode:o}");
        }
    }

    #[test]
    fn a_provider_finds_its_key_in_the_auth_file() {
        let mut auth = Auth::default();
        auth.keys.insert("MY_KEY".into(), "secret".into());
        let provider = ProviderConfig {
            name: None,
            kind: None,
            base_url: None,
            env: vec!["MY_KEY".into()],
            models: BTreeMap::new(),
        };
        // The environment has nothing, so the auth file is used.
        assert_eq!(provider.api_key_with(&auth).as_deref(), Some("secret"));
    }

    #[test]
    fn the_config_round_trips() {
        let config = Config::parse("schema = 1\ndefault_mode = 'ask'\n").expect("parses");
        assert_eq!(config.schema, 1);
        assert_eq!(config.default_mode.as_deref(), Some("ask"));
        let text = toml::to_string_pretty(&config).expect("serializes");
        let again = Config::parse(&text).expect("parses again");
        assert_eq!(config, again);
    }

    #[test]
    fn the_schema_is_optional_but_always_one_when_given() {
        let config = ProvidersConfig::parse("[provider.x]\nbase_url = 'y'\n").expect("parses");
        assert_eq!(config.schema, 1);
    }

    #[test]
    fn the_old_key_is_migrated_to_the_new_one() {
        let mut config = Config::parse("schema = 1\nllama_server_path = '/usr/bin/llama-server'\n")
            .expect("parses");
        assert!(config.migrate(), "the file has to be written again");
        assert_eq!(config.llama_path.as_deref(), Some("/usr/bin/llama-server"));
        assert_eq!(config.llama_subcommand.as_deref(), Some(""));
        assert!(config.llama_server_path.is_none());

        // A second migration changes nothing.
        assert!(!config.migrate());
    }

    #[test]
    fn the_command_of_the_server_is_the_binary_and_the_subcommand() {
        let config = Config::parse(
            "schema = 1\nllama_path = '/usr/bin/llama'\nllama_subcommand = 'serve'\n",
        )
        .expect("parses");
        let (path, subcommand) = config.llama_command();
        assert_eq!(path, "/usr/bin/llama");
        assert_eq!(subcommand.as_deref(), Some("serve"));
    }

    #[test]
    fn an_unknown_kind_is_not_a_kind() {
        assert_eq!(ProviderKind::parse("mystery"), None);
        assert_eq!(
            ProviderKind::parse("openai-chat"),
            Some(ProviderKind::OpenaiChat)
        );
    }
}
