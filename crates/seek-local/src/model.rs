//! Discovering local models (SPEC 7.1).
//!
//! On startup and whenever `/model` opens, SEEK scans `~/seek/models/` for `*.gguf`. Every file
//! becomes a model with the id `local/<file-stem>`, flagged `LOCAL`.
//!
//! Split GGUFs (`-00001-of-0000N`) list only the first shard. An optional sidecar
//! `<file-stem>.toml` holds the launch options and the assigned port.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The schema version of the sidecar.
pub const SCHEMA: u32 = 1;

/// A local model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModel {
    /// The id of the model, `local/<file-stem>`.
    pub id: String,
    /// The name shown in the picker.
    pub name: String,
    /// The file the model lives in.
    pub path: PathBuf,
    /// The launch options and the port of the model.
    pub sidecar: Sidecar,
}

impl LocalModel {
    /// The port the server of this model uses.
    pub fn port(&self) -> Option<u16> {
        self.sidecar.port
    }
}

/// The sidecar of a model: `<file-stem>.toml` next to the model file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    /// The schema version; always 1 for now.
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The port the server listens on.
    #[serde(default)]
    pub port: Option<u16>,
    /// The launch options of the user, as free text.
    #[serde(default)]
    pub options: Option<String>,
}

fn default_schema() -> u32 {
    SCHEMA
}

impl Default for Sidecar {
    fn default() -> Self {
        Sidecar {
            schema: SCHEMA,
            port: None,
            options: None,
        }
    }
}

impl Sidecar {
    /// Parse the sidecar.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Read the sidecar of a model, if it is there.
    pub fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        Sidecar::parse(&text).ok()
    }

    /// Write the sidecar.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }
}

/// The name of the sidecar of a model file.
pub fn sidecar_path(model_path: &Path) -> PathBuf {
    let stem = model_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    model_path.with_file_name(format!("{stem}.toml"))
}

/// Whether a file is the first shard of a split model.
///
/// Split GGUFs are named `model-00001-of-00003.gguf`; only the first shard is listed.
pub fn is_first_shard(name: &str) -> bool {
    match split_shard(name) {
        Some((index, _total)) => index == 1,
        None => true,
    }
}

/// The shard number and the total, if the name is a split model.
fn split_shard(name: &str) -> Option<(u32, u32)> {
    let stem = name.strip_suffix(".gguf")?;
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() < 3 {
        return None;
    }
    let total: u32 = parts[parts.len() - 1].parse().ok()?;
    if parts[parts.len() - 2] != "of" {
        return None;
    }
    let index: u32 = parts[parts.len() - 3].parse().ok()?;
    if total == 0 || index == 0 || index > total {
        return None;
    }
    Some((index, total))
}

/// Scan a folder for models.
pub fn scan(folder: &Path) -> Vec<LocalModel> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        })
        .collect();
    paths.sort();

    let mut models = Vec::new();
    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if !is_first_shard(&name) {
            // Only the first shard of a split model is listed.
            continue;
        }
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default();
        let sidecar = Sidecar::read(&sidecar_path(&path)).unwrap_or_default();
        models.push(LocalModel {
            id: format!("local/{stem}"),
            name: stem,
            path,
            sidecar,
        });
    }
    models
}

/// The models of a folder, by id.
pub fn by_id(models: &[LocalModel]) -> BTreeMap<String, LocalModel> {
    models
        .iter()
        .map(|model| (model.id.clone(), model.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seek-local-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is created");
        dir
    }

    #[test]
    fn the_folder_is_scanned_for_gguf_files() {
        let dir = temp_dir("scan");
        std::fs::write(dir.join("mistral-7b.gguf"), b"x").expect("written");
        std::fs::write(dir.join("notes.txt"), b"x").expect("written");
        std::fs::write(dir.join("other.bin"), b"x").expect("written");

        let models = scan(&dir);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "local/mistral-7b");
        assert_eq!(models[0].name, "mistral-7b");
    }

    #[test]
    fn only_the_first_shard_of_a_split_model_is_listed() {
        let dir = temp_dir("split");
        std::fs::write(dir.join("big-00001-of-00003.gguf"), b"x").expect("written");
        std::fs::write(dir.join("big-00002-of-00003.gguf"), b"x").expect("written");
        std::fs::write(dir.join("big-00003-of-00003.gguf"), b"x").expect("written");

        let models = scan(&dir);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "local/big-00001-of-00003");
    }

    #[test]
    fn a_model_that_is_not_split_is_listed() {
        assert!(is_first_shard("model.gguf"));
        assert!(is_first_shard("model-00001-of-00001.gguf"));
        assert!(!is_first_shard("model-00002-of-00003.gguf"));
        assert!(is_first_shard("model-00001-of-00003.gguf"));
    }

    #[test]
    fn the_sidecar_is_read_and_written() {
        let dir = temp_dir("sidecar");
        std::fs::write(dir.join("m.gguf"), b"x").expect("written");

        let sidecar = Sidecar {
            schema: SCHEMA,
            port: Some(8123),
            options: Some("--ctx-size 8192".into()),
        };
        sidecar
            .write(&sidecar_path(&dir.join("m.gguf")))
            .expect("written");

        let models = scan(&dir);
        assert_eq!(models[0].port(), Some(8123));
        assert_eq!(
            models[0].sidecar.options.as_deref(),
            Some("--ctx-size 8192")
        );
    }

    #[test]
    fn a_model_without_a_sidecar_still_works() {
        let dir = temp_dir("no-sidecar");
        std::fs::write(dir.join("m.gguf"), b"x").expect("written");
        let models = scan(&dir);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].port(), None);
        assert_eq!(models[0].sidecar.schema, SCHEMA);
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        assert!(scan(Path::new("/does/not/exist")).is_empty());
    }
}
