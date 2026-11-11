//! Skills (SPEC 9.5).
//!
//! A skill is a folder with a `SKILL.md`: YAML frontmatter followed by Markdown instructions.
//! The format is the one of the Agent Skills specification.
//!
//! The prompt only carries the name, the description and the path of the enabled skills — about
//! a hundred tokens each. The body is loaded when the skill is used.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The name of the file of a skill.
pub const SKILL_FILE: &str = "SKILL.md";

/// The frontmatter of a skill.
#[derive(Debug, Clone, Default, Deserialize)]
struct Frontmatter {
    /// The name of the skill: lowercase letters, numbers and hyphens.
    #[serde(default)]
    name: String,
    /// What the skill does and when to use it.
    #[serde(default)]
    description: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    compatibility: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    /// The tools the skill may use.
    #[serde(default, alias = "allowed_tools", rename = "allowed-tools")]
    allowed_tools: Option<Vec<String>>,
}

/// A skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The name, which matches the folder.
    pub name: String,
    /// What the skill does and when to use it.
    pub description: String,
    /// The folder of the skill.
    pub path: PathBuf,
    /// The instructions, without the frontmatter.
    pub body: String,
    /// The tools the skill may use.
    pub allowed_tools: Vec<String>,
    /// The license of the skill.
    pub license: Option<String>,
    /// What the skill needs to work.
    pub compatibility: Option<String>,
    /// Anything else the frontmatter carries.
    pub metadata: Option<serde_json::Value>,
}

impl Skill {
    /// The catalog entry of the skill, as it goes into the prompt.
    ///
    /// Only the name, the description and the path: the body is loaded on demand.
    pub fn catalog_entry(&self) -> String {
        format!(
            "- {}: {} (path: {})",
            self.name,
            self.description,
            self.path.display()
        )
    }
}

/// Something went wrong with a skill.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("the frontmatter of {0} is not valid: {1}")]
    Frontmatter(String, String),
    #[error("{0} has no name")]
    NoName(String),
    #[error("{0} has no description")]
    NoDescription(String),
}

/// Parse a `SKILL.md`.
pub fn parse(markdown: &str, path: &Path) -> Result<Skill, SkillError> {
    let rest = markdown.strip_prefix("---").ok_or_else(|| {
        SkillError::Frontmatter(path.display().to_string(), "no frontmatter".to_string())
    })?;
    // The frontmatter ends at the next "---" on its own line.
    let end = rest.find("\n---").ok_or_else(|| {
        SkillError::Frontmatter(
            path.display().to_string(),
            "the frontmatter is not closed".to_string(),
        )
    })?;
    let front = &rest[..end];
    let body = rest[end + 4..].trim_start_matches('\n').to_string();

    let parsed: Frontmatter = serde_norway::from_str(front)
        .map_err(|error| SkillError::Frontmatter(path.display().to_string(), error.to_string()))?;

    if parsed.name.trim().is_empty() {
        return Err(SkillError::NoName(path.display().to_string()));
    }
    if parsed.description.trim().is_empty() {
        return Err(SkillError::NoDescription(path.display().to_string()));
    }

    Ok(Skill {
        name: parsed.name.trim().to_string(),
        description: parsed.description.trim().to_string(),
        path: path.to_path_buf(),
        body,
        allowed_tools: parsed.allowed_tools.unwrap_or_default(),
        license: parsed.license,
        compatibility: parsed.compatibility,
        metadata: parsed.metadata,
    })
}

/// Scan a folder for skills.
///
/// Every folder with a `SKILL.md` is a skill; a broken one is skipped.
pub fn scan(dir: &Path) -> Vec<Skill> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();

    let mut skills = Vec::new();
    for folder in folders {
        let file = folder.join(SKILL_FILE);
        let Ok(markdown) = std::fs::read_to_string(&file) else {
            continue;
        };
        if let Ok(skill) = parse(&markdown, &file) {
            skills.push(skill);
        }
    }
    skills
}

/// The skills of a folder, by name.
pub fn by_name(skills: &[Skill]) -> std::collections::BTreeMap<String, Skill> {
    skills
        .iter()
        .map(|skill| (skill.name.clone(), skill.clone()))
        .collect()
}

/// The catalog of the enabled skills, as it goes into the prompt.
pub fn catalog(skills: &[Skill], enabled: &dyn Fn(&str) -> bool) -> String {
    let entries: Vec<String> = skills
        .iter()
        .filter(|skill| enabled(&skill.name))
        .map(Skill::catalog_entry)
        .collect();
    if entries.is_empty() {
        return String::new();
    }
    format!("Skills:\n{}", entries.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-skill-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("created");
        dir
    }

    const EXAMPLE: &str = "---\nname: init\ndescription: Write an AGENTS.md for the repository.\nlicense: MIT\nallowed-tools: [\"Read\", \"Write\"]\n---\n\n# Init\n\nInspect the repository and write the file.\n";

    #[test]
    fn a_skill_is_parsed() {
        let skill = parse(EXAMPLE, Path::new("/skills/init/SKILL.md")).expect("parsed");
        assert_eq!(skill.name, "init");
        assert_eq!(skill.description, "Write an AGENTS.md for the repository.");
        assert!(skill.body.contains("Inspect the repository"));
        assert_eq!(skill.allowed_tools, vec!["Read", "Write"]);
    }

    #[test]
    fn a_skill_without_a_name_is_rejected() {
        let text = "---\ndescription: something\n---\n\nbody";
        let error = parse(text, Path::new("/skills/x/SKILL.md")).unwrap_err();
        assert!(error.to_string().contains("no name"));
    }

    #[test]
    fn a_skill_without_a_description_is_rejected() {
        let text = "---\nname: x\n---\n\nbody";
        let error = parse(text, Path::new("/skills/x/SKILL.md")).unwrap_err();
        assert!(error.to_string().contains("no description"));
    }

    #[test]
    fn a_file_without_frontmatter_is_rejected() {
        let error = parse("just text", Path::new("/skills/x/SKILL.md")).unwrap_err();
        assert!(error.to_string().contains("frontmatter"));
    }

    #[test]
    fn the_folder_is_scanned() {
        let dir = temp_dir("scan");
        let folder = dir.join("init");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(folder.join(SKILL_FILE), EXAMPLE).expect("written");
        std::fs::create_dir_all(dir.join("empty")).expect("created");

        let skills = scan(&dir);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "init");
    }

    #[test]
    fn a_broken_skill_is_skipped() {
        let dir = temp_dir("broken");
        let folder = dir.join("broken");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(folder.join(SKILL_FILE), "no frontmatter").expect("written");

        assert!(scan(&dir).is_empty());
    }

    #[test]
    fn the_catalog_carries_only_the_name_and_the_description() {
        let dir = temp_dir("catalog");
        let folder = dir.join("init");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(folder.join(SKILL_FILE), EXAMPLE).expect("written");

        let skills = scan(&dir);
        let catalog = catalog(&skills, &|_| true);
        assert!(catalog.contains("init"));
        assert!(catalog.contains("Write an AGENTS.md"));
        assert!(
            !catalog.contains("Inspect the repository"),
            "the body stays out"
        );
    }

    #[test]
    fn a_disabled_skill_is_not_in_the_catalog() {
        let dir = temp_dir("disabled");
        let folder = dir.join("init");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(folder.join(SKILL_FILE), EXAMPLE).expect("written");

        let skills = scan(&dir);
        assert!(catalog(&skills, &|_| false).is_empty());
    }
}
