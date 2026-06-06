use std::sync::{LazyLock, OnceLock};
use std::{collections::BTreeMap, fs, io, path::PathBuf};

use crate::app::APP_HANDLE;
use crate::wallpapers::WallpaperManifest;

use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

static WATCHER: OnceLock<notify::RecommendedWatcher> = OnceLock::new();

pub static CONFIG_FILENAME: &str = "config.toml";

pub static CONFIG_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let config_dir = APP_HANDLE.path().app_config_dir().unwrap();
    fs::create_dir_all(&config_dir).expect("cannot create config dir");
    config_dir.join("config.toml")
});

pub static CONFIG: LazyLock<watch::Receiver<Config>> = LazyLock::new(|| {
    let initial = Config::load().unwrap_or_default();
    let (tx, rx) = watch::channel(initial);

    let watch_dir = APP_HANDLE.path().app_config_dir().unwrap();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };

        // Filter to only events involving config.toml.
        let involves_config = event.paths.iter().any(|p| {
            p.file_name()
                .map(|s| s.to_str() == Some(CONFIG_FILENAME))
                .unwrap_or_default()
        });
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
            Err(e) => eprintln!("underpane: config reload failed: {e}"),
        }
    })
    .unwrap();

    watcher
        .watch(&watch_dir, RecursiveMode::NonRecursive)
        .unwrap();

    WATCHER.set(watcher).unwrap();

    rx
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wallpapers_directories: Vec<String>,
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
            wallpapers_directories: Vec::new(),
            monitors: BTreeMap::new(),
        }
    }
}

pub(crate) fn expand_tilde(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        APP_HANDLE
            .path()
            .home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|_| PathBuf::from(raw))
    } else {
        PathBuf::from(raw)
    }
}

impl Config {
    /// Load config from disk, creating a default file if none exists.
    /// Any other I/O or parse error is returned as `Err`.
    pub fn load() -> Result<Self, ConfigError> {
        match fs::read_to_string(CONFIG_PATH.clone()) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save()?;
                Ok(config)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_wallpaper_dirs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .wallpapers_directories
            .iter()
            .map(|raw| expand_tilde(raw))
            .collect();

        let user_dir = APP_HANDLE.path().app_data_dir().unwrap().join("wallpapers");
        let _ = fs::create_dir_all(&user_dir);
        out.push(user_dir);

        out.push(APP_HANDLE.path().resource_dir().unwrap().join("wallpapers"));
        out
    }

    pub fn get_monitor_config(&self, index: usize) -> Option<&MonitorConfig> {
        let i1 = index + 1;
        self.monitors
            .get(&i1.to_string())
            .or(self.monitors.get("default"))
    }

    /// Scans the wallpapers directory and loads each subdirectory's `manifest.toml`.
    /// Returns a map from wallpaper directory name to its manifest.
    /// Subdirectories that are missing a manifest or have an unparseable one are silently skipped.
    /// Directory names that aren't valid wallpaper ids (see `is_valid_wallpaper_id`) are skipped.
    pub fn wallpapers(&self) -> Result<BTreeMap<String, WallpaperManifest>, ConfigError> {
        let mut map = BTreeMap::new();

        for dir in self.get_wallpaper_dirs() {
            let read = match fs::read_dir(&dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            for entry in read.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                if !ft.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if !is_valid_wallpaper_id(&name) {
                    continue;
                }
                if map.contains_key(&name) {
                    continue;
                }
                if let Ok(manifest) = WallpaperManifest::load(entry.path()) {
                    map.insert(name, manifest);
                }
            }
        }

        Ok(map)
    }

    /// Persist config to disk, creating parent directories as needed.
    pub fn save(&self) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        fs::write(CONFIG_PATH.clone(), text)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Deserialize(toml::de::Error),
    Serialize(toml::ser::Error),
    Notify(notify::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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

/// Whether `name` is usable as a wallpaper id. Wallpaper ids end up in URLs
/// (the custom protocol host is `monitor-{n}.{wallpaper}`), so they must be
/// valid hostnames: lowercase letters, digits, and hyphens, with no leading
/// or trailing hyphen and at least one character.
pub fn is_valid_wallpaper_id(name: &str) -> bool {
    if name.is_empty() || name.len() >= 64 {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn show_config_ui(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("config") {
        let _ = window.set_focus();
    } else {
        let _ = tauri::WebviewWindowBuilder::new(
            app,
            "config",
            tauri::WebviewUrl::App(std::path::PathBuf::from("index.html")),
        )
        .title("Configure")
        .inner_size(800.0, 600.0)
        .maximizable(false)
        .build();
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_wallpaper_id;

    #[test]
    fn valid_wallpaper_ids() {
        for name in ["cube", "video", "synthwave-canyon", "a1b2", "a-b-c-9"] {
            assert!(is_valid_wallpaper_id(name), "expected valid: {name}");
        }
    }

    #[test]
    fn rejects_empty_and_boundary_hyphens() {
        for name in ["", "-", "-foo", "foo-", "-foo-"] {
            assert!(!is_valid_wallpaper_id(name), "expected invalid: {name:?}");
        }
    }

    #[test]
    fn rejects_disallowed_characters() {
        for name in [
            "Foo",
            "FOO",
            "foo_bar",
            "foo bar",
            "foo.bar",
            "foo/bar",
            "foo!",
            "ünïcödé",
        ] {
            assert!(!is_valid_wallpaper_id(name), "expected invalid: {name:?}");
        }
    }
}
