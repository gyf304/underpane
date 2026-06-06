mod app;
mod config;
mod cursor_position;
mod desktop_windows;
mod handlers;
mod install;
mod locale;
mod monitor_info;
mod protocol;
mod tray;
mod utils;
mod wallpapers;
mod window_info;

pub use config::ConfigError;
use config::CONFIG;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

use crate::app::{APP_HANDLE, APP_HANDLE_LOCK};
use crate::config::show_config_ui;
use crate::desktop_windows::{sync_desktop_windows, DESKTOP_WINDOWS};
use crate::monitor_info::MONITORS;

const DEEP_LINK_SCHEME_PREFIX: &str = "underpane+https:";

/// Holds the most recent deep-link URL that hasn't yet been consumed by the
/// config UI. The frontend drains this on mount via `take_pending_install_url`
/// so a cold-start launch doesn't lose its URL to the emit-before-listener race.
pub static PENDING_INSTALL_URL: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

async fn tauri_main(app: &AppHandle) {
    let mut monitors_rx = MONITORS.clone();
    let mut config_rx = CONFIG.clone();
    tray::init();
    let _ = sync_desktop_windows();
    if DESKTOP_WINDOWS
        .lock()
        .is_ok_and(|v| v.iter().all(|w| w.is_none()))
    {
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

fn handle_install_url(app: &AppHandle, url: &str) {
    if !url.starts_with(DEEP_LINK_SCHEME_PREFIX) {
        eprintln!("underpane: ignoring deep link with unexpected scheme: {url}");
        return;
    }
    if let Ok(mut slot) = PENDING_INSTALL_URL.lock() {
        *slot = Some(url.to_string());
    }
    // Defer window creation + emit so we don't block the macOS main thread
    // while the deep-link callback is still on the stack — calling
    // WebviewWindowBuilder::build() inline from here deadlocks because it
    // needs the main thread to pump Cocoa events to finish.
    let app = app.clone();
    let url = url.to_string();
    tauri::async_runtime::spawn(async move {
        show_config_ui(&app);
        let payload = serde_json::json!({ "source_url": url });
        // If the config window hasn't finished loading yet, the emit will be
        // lost for that window — the frontend drains PENDING_INSTALL_URL on
        // mount as a backup.
        let _ = app.emit_to("config", "install-request", payload);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // With the `deep-link` feature, the plugin auto-forwards URL args
            // into the deep-link plugin's `on_open_url` listeners — no manual
            // dispatch needed here. Just focus the existing config window.
            if let Some(window) = app.get_webview_window("config") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
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
            handlers::runtime_log,
            handlers::install_wallpaper,
            handlers::take_pending_install_url,
            handlers::pick_file,
        ])
        .register_asynchronous_uri_scheme_protocol(
            "underpane",
            |_ctx, request, responder| {
                tauri::async_runtime::spawn(async move {
                    let resp = protocol::handle(request).await;
                    responder.respond(resp);
                });
            },
        )
        .setup(move |app| {
            APP_HANDLE_LOCK.set(app.handle().clone()).unwrap();

            // Register the deep-link scheme at runtime (no-op on platforms
            // where the bundler handles it, e.g. macOS via Info.plist).
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                let _ = app.deep_link().register_all();
            }

            // Listen for deep-link URL opens and forward into the install flow.
            let app_handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    handle_install_url(&app_handle, url.as_str());
                }
            });

            app.once("init", |_| {
                tauri::async_runtime::spawn(tauri_main(&APP_HANDLE));
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
