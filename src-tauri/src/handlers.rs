use std::collections::BTreeMap;
use tauri::Window;
use crate::config::{Config, CONFIG};
use crate::monitor_info::{current_monitors, MonitorInfo};
use crate::wallpapers::WallpaperManifest;

fn require_config_window(window: &Window) -> Result<(), String> {
    if window.label() == "config" {
        Ok(())
    } else {
        Err("not the config window".into())
    }
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
pub fn list_monitors(window: Window) -> Result<Vec<MonitorInfo>, String> {
    require_config_window(&window)?;
    Ok(current_monitors())
}

#[tauri::command]
pub fn list_wallpapers(window: Window) -> Result<BTreeMap<String, WallpaperManifest>, String> {
    require_config_window(&window)?;
    CONFIG.borrow().wallpapers().map_err(|e| e.to_string())
}
