use crate::config::{Config, WallpaperConfig, CONFIG, CONFIG_PATH};
use crate::desktop_windows::calc_desktop_visibility;
use crate::monitor_info::{current_monitors, MonitorInfo};
use crate::wallpapers::WallpaperManifest;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Window};
use tauri_plugin_autostart::ManagerExt;
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
    let cfg = CONFIG.borrow();
    let monitor_config = cfg
        .get_monitor_config(index)
        .ok_or_else(|| "no config for monitor".to_string())?;
    Ok(monitor_config.config.clone())
}
