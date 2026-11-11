//! The home widgets (SPEC 10.5).
//!
//! A widget is a folder `~/seek/widgets/<name>/` with a `widget.toml` (name, title, timeout) and
//! an executable `run` that prints up to eight lines of forty columns of plain text.
//!
//! SEEK runs the widgets when the home screen is shown, with a 500 ms timeout. A widget that
//! fails shows its title and a dash.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The name of the declaration of a widget.
pub const WIDGET_FILE: &str = "widget.toml";
/// The name of the executable of a widget.
pub const RUN_FILE: &str = "run";

/// How long a widget may run.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);
/// How many lines a widget may print.
pub const MAX_LINES: usize = 8;
/// How many columns a line may have.
pub const MAX_COLUMNS: usize = 40;

/// The declaration of a widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetConfig {
    /// The name of the widget.
    pub name: String,
    /// The title above the widget.
    #[serde(default)]
    pub title: String,
    /// How long the widget may run, in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl WidgetConfig {
    /// The timeout of the widget.
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(
            self.timeout_ms
                .unwrap_or(DEFAULT_TIMEOUT.as_millis() as u64),
        )
    }
}

/// A widget: the declaration and the executable.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub config: WidgetConfig,
    /// The folder of the widget.
    pub dir: PathBuf,
}

/// What a widget produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetOutput {
    /// The name of the widget.
    pub name: String,
    /// The title of the widget.
    pub title: String,
    /// The lines it printed.
    pub lines: Vec<String>,
    /// Whether the widget failed.
    pub failed: bool,
}

impl WidgetOutput {
    /// The lines of a widget that failed.
    pub fn failed(name: &str, title: &str) -> Self {
        WidgetOutput {
            name: name.to_string(),
            title: title.to_string(),
            lines: vec!["-".to_string()],
            failed: true,
        }
    }

    /// The width of the widget, in columns.
    pub fn width(&self) -> usize {
        self.lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0)
    }
}

impl Widget {
    /// Run the widget.
    ///
    /// The executable inherits the working directory, so a widget can call `git` and see the
    /// project the user is in.
    pub async fn run(&self) -> WidgetOutput {
        let program = self.dir.join(RUN_FILE);
        if !program.is_file() {
            return WidgetOutput::failed(&self.config.name, &self.config.title);
        }

        // The widget inherits the working directory of the user, so it sees the project.
        let child = match tokio::process::Command::new(&program)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(_) => {
                return WidgetOutput::failed(&self.config.name, &self.config.title);
            }
        };

        let timeout = self.config.timeout();
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            _ => {
                // The widget took too long.
                return WidgetOutput::failed(&self.config.name, &self.config.title);
            }
        };

        if !output.status.success() {
            return WidgetOutput::failed(&self.config.name, &self.config.title);
        }

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let lines: Vec<String> = text
            .lines()
            .take(MAX_LINES)
            .map(|line| {
                let trimmed: String = line.chars().take(MAX_COLUMNS).collect();
                trimmed.trim_end().to_string()
            })
            .collect();

        WidgetOutput {
            name: self.config.name.clone(),
            title: self.config.title.clone(),
            lines,
            failed: false,
        }
    }
}

/// Scan a folder for widgets.
pub fn scan(dir: &Path) -> Vec<Widget> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();

    let mut widgets = Vec::new();
    for folder in folders {
        let file = folder.join(WIDGET_FILE);
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(config) = toml::from_str::<WidgetConfig>(&text) else {
            continue;
        };
        widgets.push(Widget {
            config,
            dir: folder,
        });
    }
    widgets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-widget-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("created");
        dir
    }

    fn a_widget(dir: &Path, script: &str) -> Widget {
        let folder = dir.join("git");
        std::fs::create_dir_all(&folder).expect("created");
        std::fs::write(
            folder.join(WIDGET_FILE),
            "name = \"git\"\ntitle = \"recent commits\"\n",
        )
        .expect("written");
        std::fs::write(folder.join(RUN_FILE), script).expect("written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                folder.join(RUN_FILE),
                std::fs::Permissions::from_mode(0o755),
            );
        }
        scan(dir).into_iter().next().expect("the widget is found")
    }

    #[test]
    fn a_widget_is_scanned() {
        let dir = temp_dir("scan");
        a_widget(&dir, "#!/bin/sh\necho hi\n");
        let widgets = scan(&dir);
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].config.name, "git");
        assert_eq!(widgets[0].config.title, "recent commits");
    }

    #[tokio::test]
    async fn a_widget_prints_its_lines() {
        let dir = temp_dir("run");
        let widget = a_widget(&dir, "#!/bin/sh\necho one\necho two\n");
        let output = widget.run().await;
        assert!(!output.failed);
        assert_eq!(output.lines, vec!["one".to_string(), "two".to_string()]);
    }

    #[tokio::test]
    async fn a_failing_widget_shows_a_dash() {
        let dir = temp_dir("fail");
        let widget = a_widget(&dir, "#!/bin/sh\nexit 1\n");
        let output = widget.run().await;
        assert!(output.failed);
        assert_eq!(output.lines, vec!["-".to_string()]);
    }

    #[tokio::test]
    async fn a_widget_that_takes_too_long_fails() {
        let dir = temp_dir("slow");
        let widget = a_widget(&dir, "#!/bin/sh\nsleep 5\n");
        let output = widget.run().await;
        assert!(output.failed);
    }

    #[tokio::test]
    async fn the_output_is_capped() {
        let dir = temp_dir("cap");
        let widget = a_widget(
            &dir,
            "#!/bin/sh\nfor i in $(seq 1 20); do echo 0123456789012345678901234567890123456789; done\n",
        );
        let output = widget.run().await;
        assert_eq!(output.lines.len(), MAX_LINES);
        assert!(
            output
                .lines
                .iter()
                .all(|line| line.chars().count() <= MAX_COLUMNS)
        );
    }
}
