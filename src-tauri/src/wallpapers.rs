use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io, path::PathBuf};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperManifest {
    pub name: String,
    pub config: HashMap<String, WallpaperConfigSchema>,
}

impl WallpaperManifest {
    pub fn load(path: PathBuf) -> Result<Self, io::Error> {
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
        assert!(matches!(schema, WallpaperConfigSchema::Bool { default: Some(true), .. }));
    }
}
