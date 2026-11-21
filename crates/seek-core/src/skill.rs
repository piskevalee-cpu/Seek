//! Skills (SPEC 9.5).
//!
//! A skill is a folder with a `SKILL.md`: YAML frontmatter followed by Markdown instructions.
//! The format is the one of the Agent Skills specification.
//!
//! The prompt only carries the name, the description and the path of the enabled skills — about
//! a hundred tokens each. The body is loaded when the skill is used.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The name of the file of a skill.
pub const SKILL_FILE: &str = "SKILL.md";

/// Project-relative skill folders of the supported harnesses, in precedence order.
///
/// SEEK's own folder comes first, then the cross-client `.agents` convention, then the native
/// folders of the other harnesses. A folder is scanned recursively: Hermes and OpenCode group
/// skills in category folders, so a `SKILL.md` is not always a direct child.
pub const PROJECT_SKILL_DIRS: &[&str] = &[
    ".seek/skills",
    ".agents/skills",
    ".claude/skills",
    ".codex/skills",
    ".opencode/skills",
    ".hermes/skills",
    ".cursor/skills",
    ".gemini/skills",
    ".grok/skills",
    ".github/skills",
    ".windsurf/skills",
    ".cline/skills",
    ".roo/skills",
    ".kilocode/skills",
    ".continue/skills",
    ".openclaw/skills",
];

/// User-home-relative skill folders of the supported harnesses, in precedence order.
///
/// SEEK's own `~/seek/skills` is passed to [`discover`] separately because `SEEK_HOME` can move
/// it. These are the folders relative to the user's real home.
pub const USER_SKILL_DIRS: &[&str] = &[
    ".agents/skills",
    ".claude/skills",
    ".codex/skills",
    ".config/opencode/skills",
    ".opencode/skills",
    ".hermes/skills",
    ".cursor/skills",
    ".gemini/skills",
    ".gemini/config/skills",
    ".grok/skills",
    ".windsurf/skills",
    ".cline/skills",
    ".roo/skills",
    ".kilocode/skills",
    ".continue/skills",
    ".openclaw/skills",
];

/// Folders that are never descended into while looking for a `SKILL.md`.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    ".hub",
    "__pycache__",
    ".venv",
    "target",
];

/// How far below a skill root the scan walks. Hermes groups skills by category.
const MAX_DEPTH: usize = 4;

/// A cap on the catalog: each entry costs prompt tokens, so one run cannot grow without bound.
const MAX_SKILLS: usize = 100;

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

/// Split a `SKILL.md` into its frontmatter and its body.
fn split<'a>(markdown: &'a str, path: &Path) -> Result<(&'a str, &'a str), SkillError> {
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
    let body = rest[end + 4..].trim_start_matches('\n');
    Ok((&rest[..end], body))
}

/// Build the catalog entry out of a parsed frontmatter, without a body.
fn from_frontmatter(parsed: Frontmatter, path: &Path, body: String) -> Result<Skill, SkillError> {
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

/// Parse a `SKILL.md`.
pub fn parse(markdown: &str, path: &Path) -> Result<Skill, SkillError> {
    let (front, body) = split(markdown, path)?;
    let parsed: Frontmatter = serde_norway::from_str(front)
        .map_err(|error| SkillError::Frontmatter(path.display().to_string(), error.to_string()))?;
    from_frontmatter(parsed, path, body.to_string())
}

/// Read only the frontmatter of a skill, leaving the body empty.
///
/// Discovery needs the name, the description and the path for the prompt, not the instructions:
/// reading large bodies for up to a hundred skills at startup was wasted I/O. The body is loaded
/// by [`read_body`] when the skill actually runs.
pub fn read_meta(path: &Path) -> Result<Skill, SkillError> {
    let file = std::fs::File::open(path)
        .map_err(|error| SkillError::Frontmatter(path.display().to_string(), error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut front = String::new();
    let mut line = String::new();
    let first = reader
        .read_line(&mut line)
        .map_err(|error| SkillError::Frontmatter(path.display().to_string(), error.to_string()))?;
    if first == 0 || line.trim() != "---" {
        return Err(SkillError::Frontmatter(
            path.display().to_string(),
            "no frontmatter".to_string(),
        ));
    }
    front.push_str(&line);
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|error| {
            SkillError::Frontmatter(path.display().to_string(), error.to_string())
        })? == 0
        {
            return Err(SkillError::Frontmatter(
                path.display().to_string(),
                "the frontmatter is not closed".to_string(),
            ));
        }
        if line.trim() == "---" {
            break;
        }
        front.push_str(&line);
    }
    let parsed: Frontmatter = serde_norway::from_str(&front)
        .map_err(|error| SkillError::Frontmatter(path.display().to_string(), error.to_string()))?;
    from_frontmatter(parsed, path, String::new())
}

/// Read the body of a skill on demand.
pub fn read_body(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|markdown| {
            split(&markdown, path)
                .ok()
                .map(|(_, body)| body.to_string())
        })
        .unwrap_or_default()
}

/// Scan a folder for skills.
///
/// Every folder with a `SKILL.md` is a skill; a broken one is skipped. The walk goes a few levels
/// down to find category folders (as used by Hermes and OpenCode), skips dependency and VCS
/// folders, and does not look inside a folder that is already a skill.
pub fn scan(dir: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();
    scan_dir(dir, 0, &mut skills);
    skills
}

fn scan_dir(dir: &Path, depth: usize, skills: &mut Vec<Skill>) {
    if depth > MAX_DEPTH || skills.len() >= MAX_SKILLS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            !path.file_name().is_some_and(|name| {
                IGNORED_DIRS
                    .iter()
                    .any(|ignored| name == OsStr::new(ignored))
            })
        })
        .collect();
    folders.sort();

    let mut nested = Vec::new();
    for folder in folders {
        let file = folder.join(SKILL_FILE);
        if file.is_file() {
            // A skill is a leaf: its references/ and scripts/ are not more skills.
            if let Ok(skill) = read_meta(&file)
                && skills.len() < MAX_SKILLS
            {
                skills.push(skill);
            }
        } else {
            nested.push(folder);
        }
    }
    for folder in nested {
        scan_dir(&folder, depth + 1, skills);
    }
}

/// The home directory of the user, for the user-scoped skill folders.
///
/// `HOME` on Unix and `USERPROFILE` on Windows, then the platform fallback. Tests can avoid the
/// environment entirely by passing their own home to [`discover`].
pub fn user_home() -> Option<PathBuf> {
    #[cfg(windows)]
    let key = "USERPROFILE";
    #[cfg(not(windows))]
    let key = "HOME";
    if let Some(home) = std::env::var_os(key)
        && !home.is_empty()
    {
        return Some(PathBuf::from(home));
    }
    #[allow(deprecated)]
    std::env::home_dir()
}

/// Other `<folder>/skills` roots at a scope, for harnesses that are not in the lists above.
///
/// Only hidden folders are considered, and only if they actually have a `skills` folder. Known
/// folders are excluded: they are scanned in their documented precedence order instead.
fn other_harness_roots(base: &Path, known: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with('.'))
        })
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| IGNORED_DIRS.iter().any(|i| name == OsStr::new(i)))
        })
        .filter(|path| {
            !known.iter().any(|entry| {
                entry.split('/').next().is_some_and(|first| {
                    path.file_name()
                        .is_some_and(|name| name == OsStr::new(first))
                })
            })
        })
        .collect();
    folders.sort();
    folders
        .into_iter()
        .map(|folder| folder.join("skills"))
        .filter(|skills| skills.is_dir())
        .collect()
}

/// Discover the skills of a run from every supported location.
///
/// `root` is the project directory, `seek_home` the SEEK home (`~/seek` by default), `user_home`
/// the user's home directory and `builtin` the folder with the built-in skills.
///
/// Project skills win over user skills, SEEK's own folders win over the ones of other harnesses,
/// and installed skills win over the built-ins. A name is kept only once, so toggling a skill and
/// running `/<name>` always resolve to one body. Broken or unreadable folders are ignored.
pub fn discover(root: &Path, seek_home: &Path, user_home: &Path, builtin: &Path) -> Vec<Skill> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for relative in PROJECT_SKILL_DIRS {
        roots.push(root.join(relative));
    }
    // Harnesses added after this list was written: any hidden folder with a skills/ folder.
    roots.extend(other_harness_roots(root, PROJECT_SKILL_DIRS));
    roots.push(seek_home.join("skills"));
    for relative in USER_SKILL_DIRS {
        roots.push(user_home.join(relative));
    }
    roots.extend(other_harness_roots(user_home, USER_SKILL_DIRS));
    roots.push(builtin.to_path_buf());

    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();
    for dir in roots {
        for skill in scan(&dir) {
            if seen.insert(skill.name.clone()) {
                skills.push(skill);
            }
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
    fn discovery_reads_only_the_frontmatter() {
        let dir = temp_dir("meta-only");
        let path = dir.join("SKILL.md");
        std::fs::write(&path, EXAMPLE).expect("written");

        let meta = read_meta(&path).expect("the frontmatter is enough");
        assert_eq!(meta.name, "init");
        assert_eq!(meta.description, "Write an AGENTS.md for the repository.");
        assert_eq!(meta.allowed_tools, vec!["Read", "Write"]);
        assert!(meta.body.is_empty(), "the body is not read at discovery");

        let body = read_body(&path);
        assert!(body.contains("Inspect the repository"));
        let full = parse(EXAMPLE, &path).expect("parsed");
        assert_eq!(body, full.body, "the on-demand body matches a full parse");
    }

    #[test]
    fn a_broken_body_does_not_break_discovery() {
        // The body is never touched at discovery: even bytes that are not UTF-8 cannot stop the
        // catalog from loading, because reading stops at the closing frontmatter marker.
        let dir = temp_dir("meta-utf8");
        let path = dir.join("SKILL.md");
        let mut data = b"---\nname: odd\ndescription: An odd skill.\n---\n".to_vec();
        data.extend_from_slice(&[0xff, 0xfe, 0x00]);
        std::fs::write(&path, data).expect("written");

        let meta = read_meta(&path).expect("frontmatter is valid UTF-8");
        assert_eq!(meta.name, "odd");
        assert!(
            std::fs::read_to_string(&path).is_err(),
            "the body is not UTF-8"
        );
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

    fn write_skill(root: &Path, relative: &str, name: &str) {
        let folder = root.join(relative);
        std::fs::create_dir_all(&folder).expect("created");
        let text =
            format!("---\nname: {name}\ndescription: The {name} skill.\n---\n\nBody of {name}.\n");
        std::fs::write(folder.join(SKILL_FILE), text).expect("written");
    }

    fn names(skills: &[Skill]) -> Vec<&str> {
        skills.iter().map(|skill| skill.name.as_str()).collect()
    }

    #[test]
    fn nested_skills_are_found_but_a_skill_is_a_leaf() {
        let dir = temp_dir("nested");
        write_skill(&dir, "devops/deploy", "deploy");
        write_skill(&dir, "shallow", "shallow");
        write_skill(&dir, "shallow/references/not-a-skill", "hidden");
        write_skill(&dir, "node_modules/pkg", "dependency");
        write_skill(&dir, ".git/hooks/example", "vcs");

        let found: BTreeSet<String> = scan(&dir).iter().map(|skill| skill.name.clone()).collect();
        assert_eq!(
            found,
            BTreeSet::from(["deploy".to_string(), "shallow".to_string()])
        );
    }

    #[test]
    fn every_harness_folder_is_discovered() {
        let root = temp_dir("discover-project");
        let home = temp_dir("discover-home");
        let seek_home = temp_dir("discover-seek-home");
        let builtin = temp_dir("discover-builtin");

        write_skill(&root, ".claude/skills/claude-skill", "claude-skill");
        write_skill(&root, ".codex/skills/codex-skill", "codex-skill");
        write_skill(&root, ".hermes/skills/mlops/hermes-skill", "hermes-skill");
        write_skill(&root, ".agents/skills/agents-skill", "agents-skill");
        write_skill(&home, ".codex/skills/user-skill", "user-skill");
        write_skill(&seek_home, "skills/seek-skill", "seek-skill");
        write_skill(&builtin, "skills/builtin-skill", "builtin-skill");

        let skills = discover(&root, &seek_home, &home, &builtin);
        assert_eq!(
            names(&skills),
            vec![
                "agents-skill",
                "claude-skill",
                "codex-skill",
                "hermes-skill",
                "seek-skill",
                "user-skill",
                "builtin-skill",
            ]
        );
        assert!(
            skills[3]
                .path
                .ends_with(".hermes/skills/mlops/hermes-skill/SKILL.md"),
            "the nested skill keeps its path"
        );
    }

    #[test]
    fn project_skills_win_over_user_and_builtin_skills_of_the_same_name() {
        let root = temp_dir("precedence-project");
        let home = temp_dir("precedence-home");
        let seek_home = temp_dir("precedence-seek-home");
        let builtin = temp_dir("precedence-builtin");

        write_skill(&root, ".claude/skills/shared", "shared");
        write_skill(&root, ".seek/skills/local", "local");
        write_skill(&home, ".agents/skills/shared", "shared");
        write_skill(&seek_home, "skills/local", "local");
        write_skill(&builtin, "skills/shared", "shared");
        write_skill(&builtin, "skills/only-builtin", "only-builtin");

        let skills = discover(&root, &seek_home, &home, &builtin);
        assert_eq!(names(&skills), vec!["local", "shared", "only-builtin"]);
        assert!(
            skills[0].path.starts_with(&root),
            "SEEK's project folder wins over the SEEK home"
        );
        assert!(
            skills[1].path.starts_with(&root),
            "the project wins over the user and the built-ins"
        );
    }

    #[test]
    fn an_unlisted_harness_folder_is_also_discovered() {
        let root = temp_dir("future-project");
        let home = temp_dir("future-home");
        let seek_home = temp_dir("future-seek-home");
        let builtin = temp_dir("future-builtin");

        write_skill(&root, ".future/skills/future-skill", "future-skill");
        write_skill(&root, ".future/skills/shared", "shared");
        write_skill(&root, ".claude/skills/shared", "shared");
        write_skill(&home, ".newagent/skills/user-future", "user-future");
        write_skill(&root, "node_modules/pkg/skills/ignored", "ignored");

        let skills = discover(&root, &seek_home, &home, &builtin);
        assert_eq!(
            names(&skills),
            vec!["shared", "future-skill", "user-future"]
        );
        assert!(skills[0].path.starts_with(root.join(".claude")));
    }

    #[test]
    fn discovery_ignores_missing_folders() {
        let root = temp_dir("discover-missing");
        assert!(discover(&root, &root, &root, &root).is_empty());
    }
}
