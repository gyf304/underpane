use std::collections::BTreeMap;
use serde::Serialize;
use tauri::Window;
use crate::config::{CONFIG, Config, WallpaperConfig};
use crate::desktop_windows::calc_visibility;
use crate::monitor_info::{current_monitors, MonitorInfo};
use crate::wallpapers::WallpaperManifest;

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
        calc_visibility(index).ok_or_else(|| "no monitor at index".to_string())?;
    Ok(Visibility { coverage, focused })
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
