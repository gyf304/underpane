use std::{collections::BTreeMap, fs, io, path::PathBuf};
use std::sync::{ LazyLock, OnceLock };

use crate::wallpapers::WallpaperManifest;

use tokio::sync::watch;

use directories::ProjectDirs;
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

static WATCHER: OnceLock<notify::RecommendedWatcher> = OnceLock::new();

pub static CONFIG: LazyLock<watch::Receiver<Config>> = LazyLock::new(|| {
    let initial = Config::load().unwrap();
    let (tx, rx) = watch::channel(initial);

    let cfg_path = config_path().ok_or(ConfigError::NoConfigDir).unwrap();
    let watch_dir = cfg_path
        .parent()
        .ok_or_else(|| {
            ConfigError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "config path has no parent directory",
            ))
        }).unwrap()
        .to_owned();

    let cfg_filename = cfg_path.file_name().map(|n| n.to_owned());

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };

        // Filter to only events involving config.toml.
        let involves_config = event
            .paths
            .iter()
            .any(|p| p.file_name() == cfg_filename.as_deref());
        if !involves_config {
            return;
        }

        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any => {}
            _ => return,
        }

        match Config::load() {
            Ok(new_config) => {
                tx.send_if_modified(|current| {
                    if *current == new_config {
                        return false;
                    }
                    *current = new_config;
                    true
                });
            }
            Err(e) => eprintln!("activedesk: config reload failed: {e}"),
        }
    }).unwrap();

    watcher.watch(&watch_dir, RecursiveMode::NonRecursive).unwrap();

    WATCHER.set(watcher).unwrap();

    rx
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallpapers_directory: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub monitors: BTreeMap<String, MonitorConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    String(String),
    Number(f64),
    Bool(bool),
}

pub type WallpaperConfig = BTreeMap<String, Scalar>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub wallpaper: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: WallpaperConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wallpapers_directory: None,
            monitors: BTreeMap::new(),
        }
    }
}

/// Returns `~/Library/Application Support/.../config.toml` on macOS.
fn config_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "yifangu", "activedesk").map(|d| d.config_dir().join("config.toml"))
}

impl Config {
    /// Load config from disk, creating a default file if none exists.
    /// Any other I/O or parse error is returned as `Err`.
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path().ok_or(ConfigError::NoConfigDir)?;

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        match fs::read_to_string(&path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save()?;
                Ok(config)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Returns the wallpapers directory to use, creating it if it doesn't exist.
    ///
    /// Resolution order:
    /// 1. `wallpapers_directory` field in `config.toml` (supports leading `~`)
    /// 2. `~/Library/Application Support/.../wallpapers` (macOS default)
    /// 3. `./wallpapers` relative to the working directory (last-resort fallback)
    pub fn get_wallpapers_dir(&self) -> Result<PathBuf, ConfigError> {
        let path = if let Some(raw) = &self.wallpapers_directory {
            // Expand a leading `~` to the home directory.
            if let Some(rest) = raw.strip_prefix("~/") {
                directories::BaseDirs::new()
                    .map(|b| b.home_dir().join(rest))
                    .unwrap_or_else(|| PathBuf::from(raw))
            } else {
                PathBuf::from(raw)
            }
        } else {
            // Default: <data_dir>/wallpapers_directory  →  ~/Library/Application Support/activedesk/wallpapers
            ProjectDirs::from("com", "yifangu", "activedesk")
                .map(|d| d.data_dir().join("wallpapers"))
                .unwrap_or_else(|| PathBuf::from("wallpapers"))
        };

        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn get_monitor_config(&self, index: usize) -> Option<&MonitorConfig> {
        let i1 = index + 1;
        self.monitors.get(&i1.to_string()).or(self.monitors.get("default"))
    }

    /// Scans the wallpapers directory and loads each subdirectory's `manifest.toml`.
    /// Returns a map from wallpaper directory name to its manifest.
    /// Subdirectories that are missing a manifest or have an unparseable one are silently skipped.
    pub fn wallpapers(&self) -> Result<BTreeMap<String, WallpaperManifest>, ConfigError> {
        let dir = self.get_wallpapers_dir()?;
        let mut map = BTreeMap::new();

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("index.toml");
            match WallpaperManifest::load(manifest_path) {
                Ok(manifest) => {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    map.insert(name, manifest);
                }
                Err(_) => continue,
            }
        }

        Ok(map)
    }

    /// Persist config to disk, creating parent directories as needed.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = config_path().ok_or(ConfigError::NoConfigDir)?;

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    /// Could not determine the platform config directory.
    NoConfigDir,
    Io(io::Error),
    Deserialize(toml::de::Error),
    Serialize(toml::ser::Error),
    Notify(notify::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConfigDir => write!(f, "could not resolve config directory"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Deserialize(e) => write!(f, "TOML parse error: {e}"),
            Self::Serialize(e) => write!(f, "TOML serialize error: {e}"),
            Self::Notify(e) => write!(f, "file watcher error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::Deserialize(e)
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        Self::Serialize(e)
    }
}

impl From<notify::Error> for ConfigError {
    fn from(e: notify::Error) -> Self {
        Self::Notify(e)
    }
}
