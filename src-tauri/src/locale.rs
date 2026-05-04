use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Listener, Manager};

#[derive(Deserialize)]
#[serde(untagged)]
pub enum HashMapStrings {
    String(String),
    HashMap(HashMap<String, HashMapStrings>),
}

impl HashMapStrings {
    pub fn get(&self, path: &str) -> Option<&str> {
        let parts: Vec<&str> = path.split('.').collect();
        self.get_impl(&parts)
    }

    fn get_impl(&self, path: &[&str]) -> Option<&str> {
        match (path, self) {
            ([], Self::String(s)) => Some(s),
            ([l, ..], Self::HashMap(m)) => m.get(*l)?.get_impl(&path[1..]),
            _ => None,
        }
    }
}

pub struct Translations(HashMap<String, HashMapStrings>);

impl Translations {
    pub fn get<'a>(&'a self, id: &'a str) -> &'a str {
        if let Some(locale) = self.0.get(&*LOCALE) {
            locale.get(id).unwrap_or(id)
        } else {
            id
        }
    }
}

static LOCALES_ONCE_LOCK: OnceLock<Vec<String>> = OnceLock::new();

pub static LOCALES: LazyLock<Vec<String>> = LazyLock::new(|| LOCALES_ONCE_LOCK.wait().clone());

pub static T: LazyLock<Translations> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "en-US".to_string(),
        toml::from_str(include_str!("locales/en-US.toml")).expect("invalid en-US.toml"),
    );
    map.insert(
        "zh-CN".to_string(),
        toml::from_str(include_str!("locales/zh-CN.toml")).expect("invalid zh-CN.toml"),
    );
    Translations(map)
});

pub static LOCALE: LazyLock<String> = LazyLock::new(|| {
    LOCALES
        .iter()
        .find(|locale| T.0.contains_key(locale.as_str()))
        .cloned()
        .unwrap_or_else(|| "en-US".to_string())
});

pub fn init(app: &AppHandle) {
    let window = app.get_webview_window("background").unwrap();

    let app_clone = app.clone();
    app.once("locales", move |event| {
        let locales: Vec<String> = serde_json::from_str(event.payload()).unwrap_or_default();
        LOCALES_ONCE_LOCK.set(locales).unwrap();
        app_clone.emit("locales-configured", ()).unwrap();
    });

    let _ = window.eval("window.__TAURI__.event.emit('locales', navigator.languages)");
}
