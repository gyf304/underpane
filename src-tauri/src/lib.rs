mod config;
mod desktop_windows;
mod handlers;
mod monitor_info;
mod protocol;
mod utils;
mod window_info;
mod wallpapers;

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub use config::ConfigError;
use config::CONFIG;

use tauri::Manager;

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
    let config_dir = directories::ProjectDirs::from("com", "yifangu", "activedesk")
        .map(|d| d.config_dir().to_owned());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            handlers::read_system_config,
            handlers::write_system_config,
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
            let open_wallpapers_folder = MenuItem::with_id(
                app,
                "open_wallpapers_folder",
                "Open Wallpapers Folder",
                true,
                None::<&str>,
            )?;
            let refresh_wallpapers = MenuItem::with_id(
                app,
                "refresh_wallpapers",
                "Refresh Wallpapers",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&configure, &open_config_folder, &open_wallpapers_folder, &refresh_wallpapers, &quit])?;

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
                            .maximizable(false)
                            .build();
                        }
                    }
                    "refresh_wallpapers" => {
                        for (label, window) in app.webview_windows() {
                            if label.starts_with("monitor-") {
                                let _ = window.eval("window.location.reload()");
                            }
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
                    "open_wallpapers_folder" => {
                        use tauri_plugin_opener::OpenerExt;
                        match CONFIG.borrow().get_wallpapers_dir() {
                            Ok(dir) => {
                                if let Err(e) =
                                    app.opener().open_path(dir.to_string_lossy(), None::<&str>)
                                {
                                    eprintln!("activedesk: failed to open wallpaper folder: {e}");
                                }
                            }
                            Err(e) => {
                                eprintln!("activedesk: failed to resolve wallpaper folder: {e}");
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
