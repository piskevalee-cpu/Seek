//! Filesystem configuration (SPEC 5).
//!
//! The user's SEEK home holds the defaults. A project can override them with `.seek/` beside its
//! root: `.seek/SYSTEM.md` becomes the system prompt and `.seek/config.toml` overrides the keys
//! it sets. This keeps the public file formats unchanged; the project folder is optional.

use std::path::Path;

use crate::config::{CONFIG_FILE, Config};
use crate::prompt::{SYSTEM_FILE, SYSTEM_PROMPT};

/// The project-level configuration folder.
pub const PROJECT_DIR: &str = ".seek";

/// The system prompt for a run: project, then user home, then the compiled default.
pub fn system_prompt(user_home: &Path, project_root: &Path) -> String {
    for path in [
        project_root.join(PROJECT_DIR).join(SYSTEM_FILE),
        user_home.join(SYSTEM_FILE),
    ] {
        if let Ok(text) = std::fs::read_to_string(&path)
            && !text.trim().is_empty()
        {
            return text;
        }
    }
    SYSTEM_PROMPT.to_string()
}

/// The configuration for a run: the user's file with any project overrides applied.
pub fn config(user_home: &Path, project_root: &Path) -> Config {
    let mut config = Config::read(&user_home.join(CONFIG_FILE)).unwrap_or_default();
    let path = project_root.join(PROJECT_DIR).join(CONFIG_FILE);
    let Ok(local) = Config::read(&path) else {
        return config;
    };
    if local.theme.is_some() {
        config.theme = local.theme;
    }
    if local.default_mode.is_some() {
        config.default_mode = local.default_mode;
    }
    if local.default_model.is_some() {
        config.default_model = local.default_model;
    }
    if local.llama_path.is_some() {
        config.llama_path = local.llama_path;
    }
    if local.llama_subcommand.is_some() {
        config.llama_subcommand = local.llama_subcommand;
    }
    if local.show_reasoning.is_some() {
        config.show_reasoning = local.show_reasoning;
    }
    if local.reduce_motion.is_some() {
        config.reduce_motion = local.reduce_motion;
    }
    if !local.web_search.is_empty() {
        config.web_search = local.web_search;
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("seek-filesys-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let project = root.join("project");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&project).expect("project");
        (home, project)
    }

    #[test]
    fn project_prompt_wins_over_the_user_prompt() {
        let (home, project) = dirs("prompt");
        std::fs::write(home.join(SYSTEM_FILE), "user prompt").expect("written");
        let project_file = project.join(PROJECT_DIR).join(SYSTEM_FILE);
        std::fs::create_dir_all(project_file.parent().unwrap()).expect("folder");
        std::fs::write(&project_file, "project prompt").expect("written");
        assert_eq!(system_prompt(&home, &project), "project prompt");
        std::fs::remove_file(&project_file).expect("removed");
        assert_eq!(system_prompt(&home, &project), "user prompt");
    }

    #[test]
    fn project_config_overrides_only_its_keys() {
        let (home, project) = dirs("config");
        std::fs::write(
            home.join(CONFIG_FILE),
            "schema = 1\ndefault_mode = 'ask'\ndefault_model = 'user-model'\n",
        )
        .expect("written");
        let project_file = project.join(PROJECT_DIR).join(CONFIG_FILE);
        std::fs::create_dir_all(project_file.parent().unwrap()).expect("folder");
        std::fs::write(&project_file, "schema = 1\ndefault_mode = 'yolo'\n").expect("written");
        let config = config(&home, &project);
        assert_eq!(config.default_mode.as_deref(), Some("yolo"));
        assert_eq!(config.default_model.as_deref(), Some("user-model"));
    }
}
