use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::{
    io,
    path::{Path, PathBuf},
};

use crate::config::{Scalar, WallpaperConfig, CONFIG};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WallpaperConfigSchema {
    Bool {
        name: String,
        group: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    String {
        name: String,
        group: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    Number {
        name: String,
        group: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    Color {
        name: String,
        group: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        #[serde(default)]
        alpha: bool,
    },
    File {
        name: String,
        group: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        extensions: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperManifest {
    pub name: String,
    #[serde(default)]
    pub config: IndexMap<String, WallpaperConfigSchema>,
}

impl WallpaperManifest {
    /// Load the manifest for the wallpaper named `name`, resolved against the
    /// configured wallpapers directory.
    pub fn get(name: &str) -> Result<Self, io::Error> {
        let dirs = CONFIG.borrow().get_wallpaper_dirs();
        for base in dirs {
            let dir = base.join(name);
            if dir.exists() {
                return Self::load(dir);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("wallpaper '{name}' not found"),
        ))
    }

    /// Load the manifest for a wallpaper directory, preferring locale-specific
    /// variants. For each entry in `crate::locale::LOCALES` (e.g. `zh-CN`), tries
    /// `index.<locale>.toml` then `index.<lang>.toml` (e.g. `index.zh.toml`),
    /// finally falling back to `index.toml`.
    pub fn load(dir: PathBuf) -> Result<Self, io::Error> {
        let mut tried = HashSet::new();
        for locale in crate::locale::LOCALES.iter() {
            let specific = dir.join(format!("index.{locale}.toml"));
            if tried.insert(specific.clone()) && specific.exists() {
                return Self::load_file(&specific);
            }
            if let Some((prefix, _)) = locale.split_once('-') {
                let prefix_path = dir.join(format!("index.{prefix}.toml"));
                if tried.insert(prefix_path.clone()) && prefix_path.exists() {
                    return Self::load_file(&prefix_path);
                }
            }
        }
        Self::load_file(&dir.join("index.toml"))
    }

    /// Returns a `WallpaperConfig` populated with the manifest's declared defaults.
    /// Schema entries without a `default` are omitted.
    pub fn default_config(&self) -> WallpaperConfig {
        let mut out = WallpaperConfig::new();
        for (key, schema) in &self.config {
            let value = match schema {
                WallpaperConfigSchema::Bool {
                    default: Some(v), ..
                } => Some(Scalar::Bool(*v)),
                WallpaperConfigSchema::String {
                    default: Some(v), ..
                } => Some(Scalar::String(v.clone())),
                WallpaperConfigSchema::Number {
                    default: Some(v), ..
                } => Some(Scalar::Number(*v)),
                WallpaperConfigSchema::Color {
                    default: Some(v), ..
                } => Some(Scalar::String(v.clone())),
                _ => None,
            };
            if let Some(v) = value {
                out.insert(key.clone(), v);
            }
        }
        out
    }

    fn load_file(path: &Path) -> Result<Self, io::Error> {
        let contents = std::fs::read_to_string(path)?;
        toml::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_manifest() {
        let toml = r#"
            name = "My Wallpaper"

            [config.show_clock]
            type = "bool"
            name = "Show Clock"
            group = "General"
            default = true
        "#;

        let manifest: WallpaperManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.name, "My Wallpaper");
        let schema = manifest.config.get("show_clock").unwrap();
        assert!(matches!(
            schema,
            WallpaperConfigSchema::Bool {
                default: Some(true),
                ..
            }
        ));
    }

    #[test]
    fn test_deserialize_color_schema() {
        let toml = r##"
            name = "My Wallpaper"

            [config.tint]
            type = "color"
            name = "Tint"
            group = "Appearance"
            default = "#1e1e2e"
            alpha = true
        "##;

        let manifest: WallpaperManifest = toml::from_str(toml).unwrap();
        let schema = manifest.config.get("tint").unwrap();
        match schema {
            WallpaperConfigSchema::Color {
                default,
                alpha,
                name,
                ..
            } => {
                assert_eq!(name, "Tint");
                assert_eq!(default.as_deref(), Some("#1e1e2e"));
                assert!(*alpha);
            }
            _ => panic!("expected Color variant"),
        }
    }

    #[test]
    fn test_color_default_omitted_means_no_alpha() {
        let toml = r##"
            name = "My Wallpaper"

            [config.tint]
            type = "color"
            name = "Tint"
            group = "Appearance"
            default = "#ff00aa"
        "##;

        let manifest: WallpaperManifest = toml::from_str(toml).unwrap();
        let schema = manifest.config.get("tint").unwrap();
        assert!(matches!(
            schema,
            WallpaperConfigSchema::Color { alpha: false, .. }
        ));
    }

    #[test]
    fn test_color_default_applied_to_config() {
        let toml = r##"
            name = "My Wallpaper"

            [config.tint]
            type = "color"
            name = "Tint"
            group = "Appearance"
            default = "#1e1e2e"
        "##;

        let manifest: WallpaperManifest = toml::from_str(toml).unwrap();
        let cfg = manifest.default_config();
        assert_eq!(
            cfg.get("tint"),
            Some(&Scalar::String("#1e1e2e".to_string()))
        );
    }
}
