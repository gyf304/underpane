mod app;
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

pub use config::ConfigError;
use config::CONFIG;
use tauri::{AppHandle, Listener};

use crate::app::{APP_HANDLE, APP_HANDLE_LOCK};
use crate::config::show_config_ui;
use crate::desktop_windows::{DESKTOP_WINDOWS, sync_desktop_windows};
use crate::monitor_info::MONITORS;

async fn tauri_main(app: &AppHandle) {
    let mut monitors_rx = MONITORS.clone();
    let mut config_rx = CONFIG.clone();
    tray::init();
    let _ = sync_desktop_windows();
    if DESKTOP_WINDOWS.lock().is_ok_and(|v| v.iter().all(|w| w.is_none())) {
        show_config_ui(app);
    }

    loop {
        tokio::select! {
            Ok(_) = monitors_rx.changed() => {
                let _ = sync_desktop_windows();
            }
            Ok(_) = config_rx.changed() => {
                let _ = sync_desktop_windows();
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
            handlers::get_autostart,
            handlers::set_autostart,
        ])
        .register_asynchronous_uri_scheme_protocol(
            "underpane-wallpaper",
            |_ctx, request, responder| {
                tauri::async_runtime::spawn(async move {
                    let resp = protocol::handle(request).await;
                    responder.respond(resp);
                });
            },
        )
        .setup(move |app| {
            APP_HANDLE_LOCK.set(app.handle().clone()).unwrap();

            app.once("init", |_| {
                tauri::async_runtime::spawn(tauri_main(&APP_HANDLE));
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
