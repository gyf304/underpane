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

use std::path::PathBuf;

pub use config::ConfigError;
use config::CONFIG;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

use crate::app::{APP_HANDLE, APP_HANDLE_LOCK};
use crate::config::show_config_ui;
use crate::desktop_windows::{sync_desktop_windows, DESKTOP_WINDOWS};
use crate::monitor_info::MONITORS;

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
    // OS scheme/file-association registration already gates which URLs reach us.
    // Defer to a task: building a window inline from the deep-link callback
    // deadlocks the macOS main thread (it can't pump Cocoa events to finish).
    let app = app.clone();
    let url = url.to_string();
    tauri::async_runtime::spawn(async move {
        show_config_ui(&app);
        // Let a freshly-created window attach its listener before we emit; no
        // readiness signal to await, so use a fixed delay.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let payload = serde_json::json!({ "source_url": url });
        let _ = app.emit_to("config", "install-request", payload);
    });
}

/// Windows/Linux deliver a file association as an argv entry, not a
/// `RunEvent::Opened` URL; pick the first existing-file arg as a `file://` URL.
fn first_file_arg_url<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    args.into_iter()
        .skip(1)
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .and_then(|p| url::Url::from_file_path(p).ok())
        .map(|u| u.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // The deep-link feature auto-forwards URL-scheme args, but not a bare
            // file path (Windows/Linux file association) — handle that manually.
            if let Some(url) = first_file_arg_url(argv) {
                handle_install_url(app, &url);
            }
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
            handlers::pick_file,
            handlers::pick_directory,
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
            // macOS file-association opens also arrive here as `file://` URLs.
            let app_handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    handle_install_url(&app_handle, url.as_str());
                }
            });

            // Windows/Linux cold start: the `.underpane` path is passed as a CLI
            // argument rather than delivered via `RunEvent::Opened`.
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                if let Some(url) = first_file_arg_url(std::env::args()) {
                    handle_install_url(app.handle(), &url);
                }
            }

            app.once("init", |_| {
                tauri::async_runtime::spawn(tauri_main(&APP_HANDLE));
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
