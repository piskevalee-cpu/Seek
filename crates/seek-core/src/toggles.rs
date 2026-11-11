//! The toggles (SPEC 9.6).
//!
//! `state/toggles.toml` stores whether a tool or a skill is enabled. The defaults: the four
//! built-in tools are on, the read-only helpers are off, and every installed skill is on.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The schema version of the toggles.
pub const SCHEMA: u32 = 1;

/// The tools that are on when nothing was chosen (SPEC 9.2).
///
/// The read-only helpers and the network tools are off until the user turns them on.
pub const DEFAULT_ON: [&str; 4] = ["read", "write", "edit", "bash"];

/// Which tools and skills are enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toggles {
    /// The schema version; always 1 for now.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The tools that are off. A tool that is not in here is on.
    #[serde(default)]
    pub tools: BTreeMap<String, bool>,
    /// The skills that are off. A skill that is not in here is on.
    #[serde(default)]
    pub skills: BTreeMap<String, bool>,
    /// The reasoning level of every model (SPEC 9.7).
    #[serde(default)]
    pub reasoning: BTreeMap<String, String>,
}

fn default_schema() -> u32 {
    SCHEMA
}

impl Default for Toggles {
    fn default() -> Self {
        Toggles {
            schema: SCHEMA,
            tools: BTreeMap::new(),
            skills: BTreeMap::new(),
            reasoning: BTreeMap::new(),
        }
    }
}

impl Toggles {
    /// Whether a tool is enabled.
    ///
    /// A tool that was never mentioned follows the default: the four built-in tools are on, the
    /// helpers and the network tools are off (SPEC 9.2).
    pub fn tool_enabled(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .copied()
            .unwrap_or_else(|| DEFAULT_ON.contains(&name))
    }

    /// Whether a skill is enabled.
    pub fn skill_enabled(&self, name: &str) -> bool {
        self.skills.get(name).copied().unwrap_or(true)
    }

    /// Turn a tool on or off.
    pub fn set_tool(&mut self, name: &str, enabled: bool) {
        self.tools.insert(name.to_string(), enabled);
    }

    /// Turn a skill on or off.
    pub fn set_skill(&mut self, name: &str, enabled: bool) {
        self.skills.insert(name.to_string(), enabled);
    }

    /// The reasoning level of a model, if one was chosen.
    pub fn reasoning(&self, model: &str) -> Option<String> {
        self.reasoning.get(model).cloned()
    }

    /// Remember the reasoning level of a model.
    pub fn set_reasoning(&mut self, model: &str, level: &str) {
        self.reasoning.insert(model.to_string(), level.to_string());
    }

    /// The tools that are off.
    pub fn disabled_tools(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|(_, enabled)| !**enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// The skills that are off.
    pub fn disabled_skills(&self) -> Vec<String> {
        self.skills
            .iter()
            .filter(|(_, enabled)| !**enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }

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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_built_in_tools_are_on_by_default() {
        let toggles = Toggles::default();
        assert!(toggles.tool_enabled("read"));
        assert!(toggles.tool_enabled("write"));
        assert!(toggles.tool_enabled("edit"));
        assert!(toggles.tool_enabled("bash"));
        assert!(toggles.skill_enabled("init"));
        assert!(toggles.disabled_tools().is_empty());
    }

    #[test]
    fn the_helpers_and_the_network_tools_are_off_by_default() {
        let toggles = Toggles::default();
        assert!(!toggles.tool_enabled("ls"));
        assert!(!toggles.tool_enabled("grep"));
        assert!(!toggles.tool_enabled("find"));
        assert!(!toggles.tool_enabled("fetch"));
    }

    #[test]
    fn a_tool_can_be_turned_off() {
        let mut toggles = Toggles::default();
        toggles.set_tool("bash", false);
        assert!(!toggles.tool_enabled("bash"));
        assert!(toggles.tool_enabled("read"));
        assert_eq!(toggles.disabled_tools(), vec!["bash".to_string()]);
    }

    #[test]
    fn a_skill_can_be_turned_off() {
        let mut toggles = Toggles::default();
        toggles.set_skill("init", false);
        assert!(!toggles.skill_enabled("init"));
        assert_eq!(toggles.disabled_skills(), vec!["init".to_string()]);
    }

    #[test]
    fn the_reasoning_level_of_a_model_is_remembered() {
        let mut toggles = Toggles::default();
        assert_eq!(toggles.reasoning("claude-sonnet-4-5"), None);
        toggles.set_reasoning("claude-sonnet-4-5", "high");
        assert_eq!(
            toggles.reasoning("claude-sonnet-4-5").as_deref(),
            Some("high")
        );
        assert_eq!(toggles.reasoning("gpt-5.4"), None);
    }

    #[test]
    fn the_file_round_trips() {
        let mut toggles = Toggles::default();
        toggles.set_tool("bash", false);
        let text = toml::to_string_pretty(&toggles).expect("serialized");
        let again = Toggles::parse(&text).expect("parsed");
        assert_eq!(toggles, again);
    }

    #[test]
    fn the_file_is_written_and_read() {
        let dir = std::env::temp_dir().join("seek-toggles-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state").join("toggles.toml");

        let mut toggles = Toggles::default();
        toggles.set_skill("init", false);
        toggles.write(&path).expect("written");

        let read = Toggles::read(&path).expect("read");
        assert!(!read.skill_enabled("init"));
    }
}
