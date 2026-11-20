//! The settings of the user (SPEC 9.5).
//!
//! The name of the user is asked for on the first run and can be changed with `/settings`.
//! The model is told the name, and it addresses the user by it.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file the settings live in, inside the state of the SEEK home.
pub const USER_FILE: &str = "user.toml";

/// The block types a sidebar slot can hold.
pub const SIDEBAR_BLOCKS: [&str; 5] = ["sessions", "hardware", "processes", "widgets", "none"];

/// The three stacked blocks of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarLayout {
    /// The top block.
    #[serde(default = "default_block_1")]
    pub block1: String,
    /// The middle block.
    #[serde(default = "default_block_2")]
    pub block2: String,
    /// The bottom block.
    #[serde(default = "default_block_3")]
    pub block3: String,
}

fn default_block_1() -> String {
    "sessions".to_string()
}

fn default_block_2() -> String {
    "hardware".to_string()
}

fn default_block_3() -> String {
    "processes".to_string()
}

impl Default for SidebarLayout {
    fn default() -> Self {
        SidebarLayout {
            block1: default_block_1(),
            block2: default_block_2(),
            block3: default_block_3(),
        }
    }
}

impl SidebarLayout {
    /// The three blocks, top to bottom.
    pub fn blocks(&self) -> [&str; 3] {
        [&self.block1, &self.block2, &self.block3]
    }

    /// Set a slot, ignoring an unknown block name.
    pub fn set(&mut self, slot: usize, block: &str) {
        if !SIDEBAR_BLOCKS.contains(&block) || slot > 2 {
            return;
        }
        match slot {
            0 => self.block1 = block.to_string(),
            1 => self.block2 = block.to_string(),
            _ => self.block3 = block.to_string(),
        }
    }
}

/// The settings of the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSettings {
    /// The schema version; always 1 for now.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The name of the user. The model addresses them by it.
    #[serde(default)]
    pub name: String,
    /// Which side of the screen the sidebar sits on. The scrollbar is opposite (SPEC 10.3).
    #[serde(default)]
    pub sidebar_side: Side,
    /// Which blocks the three stacked sidebar slots show (SPEC 10.3).
    #[serde(default)]
    pub sidebar_blocks: SidebarLayout,
    /// Whether the sidebar was open at shutdown, restored on the next start.
    #[serde(default)]
    pub sidebar_open: bool,
}

/// The schema version of the settings.
fn default_schema() -> u32 {
    1
}

/// Which side of the screen the sidebar sits on.
///
/// The transcript scrollbar is drawn on the opposite side (SPEC 10.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The sidebar is on the left, the scrollbar on the right.
    #[default]
    Left,
    /// The sidebar is on the right, the scrollbar on the left.
    Right,
}

impl Side {
    /// The name written to the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }

    /// The other side.
    pub fn opposite(self) -> Self {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    /// Parse a name from the settings or the picker.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            _ => None,
        }
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        UserSettings {
            schema: 1,
            name: String::new(),
            sidebar_side: Side::Left,
            sidebar_blocks: SidebarLayout::default(),
            sidebar_open: false,
        }
    }
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
    fn the_settings_carry_the_schema() {
        let settings = UserSettings::default();
        assert_eq!(settings.schema, 1);
    }

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
            sidebar_side: Side::Right,
            sidebar_blocks: SidebarLayout::default(),
            sidebar_open: false,
        };
        settings.write(&path).expect("written");
        let read = UserSettings::read(&path);
        assert_eq!(read.name, "Ada");
        assert_eq!(read.sidebar_side, Side::Right);
        assert!(read.has_name());
    }

    #[test]
    fn an_old_file_without_a_side_gets_the_default() {
        let dir = std::env::temp_dir().join("seek-user-side-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the folder is created");
        let path = dir.join(USER_FILE);
        std::fs::write(&path, "schema = 1\nname = 'Ada'\n").expect("written");
        let read = UserSettings::read(&path);
        assert_eq!(read.name, "Ada");
        assert_eq!(read.sidebar_side, Side::Left);
        assert_eq!(Side::Right.opposite(), Side::Left);
        assert_eq!(Side::parse("RIGHT"), Some(Side::Right));
    }

    #[test]
    fn the_prompt_line_names_the_user() {
        let settings = UserSettings {
            schema: 1,
            name: "Ada".into(),
            sidebar_side: Side::Left,
            sidebar_blocks: SidebarLayout::default(),
            sidebar_open: false,
        };
        let line = settings.prompt_line().expect("a line");
        assert!(line.contains("Ada"), "{line}");

        // Without a name there is no line.
        let empty = UserSettings::default();
        assert!(empty.prompt_line().is_none());
    }
}
