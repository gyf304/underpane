use crate::config::{is_valid_wallpaper_id, Config, WallpaperConfig, CONFIG, CONFIG_PATH};
use crate::desktop_windows::{calc_desktop_visibility, DESKTOP_WINDOWS};
use crate::install;
use crate::monitor_info::{current_monitors, MonitorInfo};
use crate::wallpapers::WallpaperManifest;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Webview, Window};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

fn require_config_window(window: &Window) -> Result<(), String> {
    if window.label() == "config" {
        Ok(())
    } else {
        Err("not the config window".into())
    }
}

fn monitor_index_from_label(window: &Window) -> Result<usize, String> {
    let label = window.label();
    let i1: usize = label
        .strip_prefix("monitor-")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "not a monitor window".to_string())?;
    if i1 == 0 {
        return Err("invalid monitor index".into());
    }
    Ok(i1 - 1)
}

#[tauri::command]
pub fn read_system_config(window: Window) -> Result<Config, String> {
    require_config_window(&window)?;
    Ok(CONFIG.borrow().clone())
}

#[tauri::command]
pub fn write_system_config(window: Window, data: Config) -> Result<(), String> {
    require_config_window(&window)?;
    data.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_config_file(app: AppHandle, window: Window) -> Result<(), String> {
    require_config_window(&window)?;
    app.opener()
        .open_path(CONFIG_PATH.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_wallpapers_dir(app: AppHandle, window: Window) -> Result<(), String> {
    require_config_window(&window)?;
    let dirs = CONFIG.borrow().get_wallpaper_dirs();
    let dir = dirs
        .first()
        .ok_or_else(|| "no wallpapers directory".to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_monitors(window: Window) -> Result<Vec<MonitorInfo>, String> {
    require_config_window(&window)?;
    Ok(current_monitors())
}

#[tauri::command]
pub fn list_wallpapers(window: Window) -> Result<BTreeMap<String, WallpaperManifest>, String> {
    require_config_window(&window)?;
    CONFIG.borrow().wallpapers().map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct Visibility {
    pub coverage: f64,
    pub focused: bool,
}

#[tauri::command]
pub fn get_visibility(window: Window) -> Result<Visibility, String> {
    let index = monitor_index_from_label(&window)?;
    let (coverage, focused) =
        calc_desktop_visibility(index).ok_or_else(|| "no monitor at index".to_string())?;
    Ok(Visibility { coverage, focused })
}

#[tauri::command]
pub fn get_autostart(app: AppHandle, window: Window) -> Result<bool, String> {
    require_config_window(&window)?;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, window: Window, enabled: bool) -> Result<(), String> {
    require_config_window(&window)?;
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn get_config(window: Window) -> Result<WallpaperConfig, String> {
    let index = monitor_index_from_label(&window)?;
    // Reuse DesktopWindow::monitor_config so the initial load goes through the
    // same manifest-default merge and `file`-input proxy-path rewrite as the
    // `config-change` event.
    let windows = DESKTOP_WINDOWS.lock().map_err(|e| e.to_string())?;
    let desktop_window = windows
        .get(index)
        .and_then(|w| w.as_ref())
        .ok_or_else(|| "no window for monitor".to_string())?;
    let monitor_config = desktop_window
        .monitor_config()
        .ok_or_else(|| "no config for monitor".to_string())?;
    Ok(monitor_config.config)
}

#[tauri::command]
pub fn take_pending_install_url(window: Window) -> Result<Option<String>, String> {
    require_config_window(&window)?;
    Ok(crate::PENDING_INSTALL_URL.lock().ok().and_then(|mut s| s.take()))
}

#[tauri::command]
pub async fn install_wallpaper(
    app: AppHandle,
    window: Window,
    name: String,
    zip_url: String,
    install_id: String,
) -> Result<(), String> {
    require_config_window(&window)?;
    if !is_valid_wallpaper_id(&name) {
        return Err(format!(
            "invalid wallpaper name '{name}' (allowed: lowercase letters, digits, '-'; cannot start or end with '-')"
        ));
    }
    install::install_wallpaper(app, name, zip_url, install_id).await
}

#[tauri::command]
pub async fn pick_file(
    app: AppHandle,
    window: Window,
    extensions: Option<Vec<String>>,
) -> Result<Option<String>, String> {
    require_config_window(&window)?;

    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut builder = app.dialog().file();
    if let Some(exts) = &extensions {
        if !exts.is_empty() {
            let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
            builder = builder.add_filter("", &refs);
        }
    }
    builder.pick_file(move |path| {
        let _ = tx.send(path);
    });

    let picked = rx.await.map_err(|e| e.to_string())?;
    Ok(picked
        .and_then(|fp| fp.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn runtime_log(webview: Webview, level: String, message: String) {
    let line = format!("[{} {level}] {message}", webview.label());
    if level == "warn" || level == "error" {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}
