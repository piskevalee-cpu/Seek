//! The settings of the user (SPEC 9.5).
//!
//! The name of the user is asked for on the first run and can be changed with `/settings`.
//! The model is told the name, and it addresses the user by it.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file the settings live in, inside the state of the SEEK home.
pub const USER_FILE: &str = "user.toml";

/// The settings of the user.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSettings {
    /// The schema version; always 1 for now.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The name of the user. The model addresses them by it.
    #[serde(default)]
    pub name: String,
}

/// The schema version of the settings.
fn default_schema() -> u32 {
    1
}

impl UserSettings {
    /// Read the settings. A file that is not there gives the defaults.
    pub fn read(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return UserSettings::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Write the settings.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }

    /// Whether the name is known.
    pub fn has_name(&self) -> bool {
        !self.name.trim().is_empty()
    }

    /// The line that goes into the prompt, when the name is known.
    pub fn prompt_line(&self) -> Option<String> {
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        Some(format!(
            "The user is called {name}. Address them by that name."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_are_read_and_written() {
        let dir = std::env::temp_dir().join("seek-user-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is created");
        let path = dir.join(USER_FILE);

        // A file that is not there gives the defaults.
        let settings = UserSettings::read(&path);
        assert!(!settings.has_name());

        let settings = UserSettings {
            schema: 1,
            name: "Ada".into(),
        };
        settings.write(&path).expect("written");
        let read = UserSettings::read(&path);
        assert_eq!(read.name, "Ada");
        assert!(read.has_name());
    }

    #[test]
    fn the_prompt_line_names_the_user() {
        let settings = UserSettings {
            schema: 1,
            name: "Ada".into(),
        };
        let line = settings.prompt_line().expect("a line");
        assert!(line.contains("Ada"), "{line}");

        // Without a name there is no line.
        let empty = UserSettings::default();
        assert!(empty.prompt_line().is_none());
    }
}
