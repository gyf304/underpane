mod config;
mod desktop_windows;
mod handlers;
mod locale;
mod monitor_info;
mod protocol;
mod tray;
mod utils;
mod wallpapers;
mod window_info;

use std::sync::{LazyLock, Mutex};

pub use config::ConfigError;
use config::CONFIG;

use crate::desktop_windows::DesktopWindow;
use crate::monitor_info::MONITORS;

static DESKTOP_WINDOWS: LazyLock<Mutex<Vec<Option<DesktopWindow>>>> =
    LazyLock::new(|| Mutex::new(vec![]));

pub fn sync_desktop_windows(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    let mut windows = DESKTOP_WINDOWS.lock().unwrap();
    let monitor_count = MONITORS.borrow().len();
    let config = CONFIG.borrow().clone();

    windows.resize(monitor_count, None);

    for i in 0..monitor_count {
        let monitor_config = config.get_monitor_config(i);
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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            handlers::read_system_config,
            handlers::write_system_config,
            handlers::open_config_file,
            handlers::open_wallpapers_dir,
            handlers::list_monitors,
            handlers::list_wallpapers,
            handlers::get_visibility,
            handlers::get_config,
        ])
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
            let handle = app.handle().clone();

            config::init(&handle);
            monitor_info::init(&handle);
            locale::init(&handle);
            tray::init(&handle);

            sync_desktop_windows(&handle)?;

            tauri::async_runtime::spawn(async move {
                let mut monitors_rx = MONITORS.clone();
                let mut config_rx = CONFIG.clone();
                loop {
                    tokio::select! {
                        Ok(_) = monitors_rx.changed() => {
                            if let Err(e) = sync_desktop_windows(&handle) {
                                eprintln!("Error syncing windows: {e}");
                            }
                        }
                        Ok(_) = config_rx.changed() => {
                            if let Err(e) = sync_desktop_windows(&handle) {
                                eprintln!("Error syncing windows: {e}");
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
