//! Permissions (SPEC 9.4).
//!
//! The mode decides what happens to a tool call:
//!
//! | Mode   | read | write | exec | network |
//! | ------ | ---- | ----- | ---- | ------- |
//! | `read` | allow | deny  | deny | deny    |
//! | `ask`  | allow | ask   | ask  | ask     |
//! | `yolo` | allow | allow | allow| allow   |
//!
//! Paths outside the project root are always asked in `ask` mode.

use std::path::Path;

use crate::tool::Risk;

/// The permission mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// Only reading is allowed.
    Read,
    /// Everything is asked.
    Ask,
    /// Everything is allowed.
    Yolo,
}

impl PermissionMode {
    /// The text of the chip in the footer.
    pub fn label(self) -> &'static str {
        match self {
            PermissionMode::Read => "READ",
            PermissionMode::Ask => "ASK",
            PermissionMode::Yolo => "YOLO",
        }
    }

    /// The next mode, cycling read -> ask -> yolo.
    pub fn next(self) -> Self {
        match self {
            PermissionMode::Read => PermissionMode::Ask,
            PermissionMode::Ask => PermissionMode::Yolo,
            PermissionMode::Yolo => PermissionMode::Read,
        }
    }

    /// The mode of a name, if it is one.
    pub fn parse(mode: &str) -> Option<Self> {
        match mode.to_lowercase().as_str() {
            "read" => Some(PermissionMode::Read),
            "ask" => Some(PermissionMode::Ask),
            "yolo" => Some(PermissionMode::Yolo),
            _ => None,
        }
    }

    /// What happens to a tool call in this mode.
    pub fn decide(self, risk: Risk) -> Decision {
        match (self, risk) {
            (PermissionMode::Yolo, _) => Decision::Allow,
            // Reading is always allowed.
            (_, Risk::Read) => Decision::Allow,
            (PermissionMode::Read, _) => Decision::Deny,
            (PermissionMode::Ask, _) => Decision::Ask,
        }
    }

    /// What happens to a tool call that touches a path outside the project root.
    ///
    /// In `ask` mode such a path is always asked, whatever the risk is.
    pub fn decide_path(self, risk: Risk, inside_project: bool) -> Decision {
        if self == PermissionMode::Ask && !inside_project {
            return Decision::Ask;
        }
        self.decide(risk)
    }
}

/// What the mode says about a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Run it.
    Allow,
    /// Do not run it.
    Deny,
    /// Ask the user.
    Ask,
}

/// What the user decided about a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Run it.
    Allow,
    /// Do not run it.
    Deny,
    /// "Tell SEEK what to do": the text goes back to the model as the tool result.
    Tell(String),
}

impl PermissionDecision {
    /// The text handed back to the model as the tool result.
    pub fn denial_text(&self) -> String {
        match self {
            PermissionDecision::Allow => String::new(),
            PermissionDecision::Deny => "denied by user".to_string(),
            PermissionDecision::Tell(text) => format!("denied by user: {text}"),
        }
    }
}

/// Whether a path is inside the project root.
pub fn is_inside_project(root: &Path, path: &Path) -> bool {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrix_matches_the_spec() {
        assert_eq!(PermissionMode::Read.decide(Risk::Read), Decision::Allow);
        assert_eq!(PermissionMode::Read.decide(Risk::Write), Decision::Deny);
        assert_eq!(PermissionMode::Read.decide(Risk::Exec), Decision::Deny);
        assert_eq!(PermissionMode::Read.decide(Risk::Network), Decision::Deny);

        assert_eq!(PermissionMode::Ask.decide(Risk::Read), Decision::Allow);
        assert_eq!(PermissionMode::Ask.decide(Risk::Write), Decision::Ask);
        assert_eq!(PermissionMode::Ask.decide(Risk::Exec), Decision::Ask);
        assert_eq!(PermissionMode::Ask.decide(Risk::Network), Decision::Ask);

        assert_eq!(PermissionMode::Yolo.decide(Risk::Read), Decision::Allow);
        assert_eq!(PermissionMode::Yolo.decide(Risk::Write), Decision::Allow);
        assert_eq!(PermissionMode::Yolo.decide(Risk::Exec), Decision::Allow);
        assert_eq!(PermissionMode::Yolo.decide(Risk::Network), Decision::Allow);
    }

    #[test]
    fn a_path_outside_the_project_is_always_asked() {
        let root = Path::new("/tmp/project");
        assert!(is_inside_project(
            root,
            Path::new("/tmp/project/src/main.rs")
        ));
        assert_eq!(
            PermissionMode::Ask.decide_path(Risk::Read, false),
            Decision::Ask
        );
        assert_eq!(
            PermissionMode::Ask.decide_path(Risk::Read, true),
            Decision::Allow
        );
    }

    #[test]
    fn the_mode_cycles_and_parses() {
        assert_eq!(PermissionMode::Read.next(), PermissionMode::Ask);
        assert_eq!(PermissionMode::Ask.next(), PermissionMode::Yolo);
        assert_eq!(PermissionMode::Yolo.next(), PermissionMode::Read);
        assert_eq!(PermissionMode::parse("ask"), Some(PermissionMode::Ask));
        assert_eq!(PermissionMode::parse("YOLO"), Some(PermissionMode::Yolo));
        assert_eq!(PermissionMode::parse("nonsense"), None);
    }

    #[test]
    fn the_tell_decision_becomes_the_tool_result() {
        assert_eq!(
            PermissionDecision::Tell("use the other file".into()).denial_text(),
            "denied by user: use the other file"
        );
        assert_eq!(PermissionDecision::Deny.denial_text(), "denied by user");
    }
}
