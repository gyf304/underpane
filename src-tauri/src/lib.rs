mod config;
mod desktop_windows;
mod monitor_info;
mod protocol;
mod utils;
mod window_info;

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub use config::ConfigError;
use config::CONFIG;

use tauri::Manager;

use crate::desktop_windows::DesktopWindow;
use crate::monitor_info::MONITORS;

static DESKTOP_WINDOWS: LazyLock<Mutex<Vec<Option<DesktopWindow>>>> =
    LazyLock::new(|| Mutex::new(vec![]));

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn get_config(window: tauri::Window) -> Result<toml::Table, serde_json::Value> {
    let label = window.label();
    let err = serde_json::json!(["Not a monitor window"]);
    let idstr = label.strip_prefix("monitor-").ok_or(err.clone())?;
    let index = idstr.parse::<usize>().map_err(|_| err.clone())?;
    if index < 1 {
        return Err(serde_json::json!(["Not a monitor window"]));
    }
    let index = index - 1;
    let config = CONFIG.borrow();
    let monitor_config = config.get_monitor(index).ok_or(err.clone())?;
    Ok(monitor_config.config.clone())
}

pub fn sync_desktop_windows(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    let mut windows = DESKTOP_WINDOWS.lock().unwrap();
    let monitor_count = MONITORS.borrow().len();
    let config = CONFIG.borrow();

    windows.resize(monitor_count, None);

    for i in 0..monitor_count {
        let monitor_config = config.get_monitor(i);
        if monitor_config.is_some() {
            if windows[i].is_none() {
                windows[i] = Some(DesktopWindow::new(app, i)?);
            }
        } else {
            windows[i] = None;
        }
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = directories::ProjectDirs::from("com", "yifangu", "activedesk")
        .map(|d| d.config_dir().to_owned());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![get_config])
        .register_asynchronous_uri_scheme_protocol(
            "activedesk-wallpaper",
            |_ctx, request, responder| {
                tauri::async_runtime::spawn(async move {
                    let resp = protocol::handle(request).await;
                    responder.respond(resp);
                });
            },
        )
        .setup(move |app| {
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };

            let configure = MenuItem::with_id(app, "configure", "Configure", true, None::<&str>)?;
            let open_config_folder = MenuItem::with_id(
                app,
                "open_config_folder",
                "Open Config Folder",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&configure, &open_config_folder, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "configure" => {
                        if let Some(window) = app.get_webview_window("config") {
                            let _ = window.set_focus();
                        } else {
                            let _ = tauri::WebviewWindowBuilder::new(
                                app,
                                "config",
                                tauri::WebviewUrl::App(PathBuf::from("index.html")),
                            )
                            .title("Configure")
                            .inner_size(800.0, 600.0)
                            .build();
                        }
                    }
                    "open_config_folder" => {
                        use tauri_plugin_opener::OpenerExt;
                        if let Some(dir) = &config_dir {
                            if let Err(e) =
                                app.opener().open_path(dir.to_string_lossy(), None::<&str>)
                            {
                                eprintln!("activedesk: failed to open config folder: {e}");
                            }
                        }
                    }
                    "open_devtools" => {
                        app.webview_windows()
                            .values()
                            .for_each(|w| w.open_devtools());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            let handle = app.handle().clone();
            monitor_info::init(&handle);

            sync_desktop_windows(&handle)?;

            tauri::async_runtime::spawn(async move {
                let mut monitors_rx = MONITORS.clone();
                let mut config_rx = CONFIG.clone();
                loop {
                    tokio::select! {
                        Ok(_) = monitors_rx.changed() => {
                            let _ = sync_desktop_windows(&handle);
                        }
                        Ok(_) = config_rx.changed() => {
                            let _ = sync_desktop_windows(&handle);
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
