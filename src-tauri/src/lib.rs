mod config;
mod desktop_windows;
mod protocol;
mod window_info;
use config::CONFIG;
pub use config::ConfigError;
use desktop_windows::spawn_window;

use tauri::Manager;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};
use tokio::sync::broadcast;

use desktop_windows::AppEvent;
use window_info::get_all_windows;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
// #[tauri::command]
// fn get_config(window: tauri::Window) -> toml::Table {
//     let label = window.label();
// }

/// Create desktop windows for every (config entry, monitor) pair that doesn't
/// already have a window. Destruction is self-managed by each [`DesktopWindow::run`] task.
fn create_missing_windows(
    app: &tauri::AppHandle,
    config: &config::Config,
    monitors: &[tauri::Monitor],
    event_tx: broadcast::Sender<AppEvent>,
) {
    for (i, monitor) in monitors.iter().enumerate() {
        let i1 = i + 1;
        if app.webview_windows().contains_key(&format!("monitor-{i1}")) {
            continue;
        }
        let Some(wp) = config.monitors.get(&i1.to_string()) else {
            continue;
        };
        spawn_window(app, i, monitor.clone(), wp.clone(), event_tx.clone());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = directories::ProjectDirs::from("com", "yifangu", "activedesk")
        .map(|d| d.config_dir().to_owned());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // .invoke_handler(tauri::generate_handler![greet])
        .register_asynchronous_uri_scheme_protocol(
            "activedesk-wallpaper",
            |_ctx, request, responder| {
                tauri::async_runtime::spawn(async move {
                    let resp = protocol::handle(request).await;
                    responder.respond(resp);
                });
            }
        )
        .setup(move |app| {
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };

            let open_config =
                MenuItem::with_id(app, "open_config", "Open Config Folder", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_config, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "open_config" => {
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

            // Snapshot the initial monitor list.
            let initial_monitors = app.available_monitors().unwrap_or_default();

            // Unified event channel shared by all window tasks and the orchestrator.
            let (event_tx, _) = broadcast::channel::<AppEvent>(16);

            // Initial window creation.
            create_missing_windows(
                app.handle(),
                &CONFIG.borrow().clone(),
                &initial_monitors,
                event_tx.clone(),
            );

            // Bridge: forward config watch changes into the event channel.
            let mut cfg_watch = CONFIG.clone();
            let tx_cfg = event_tx.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    if cfg_watch.changed().await.is_err() {
                        break;
                    }
                    let cfg = cfg_watch.borrow_and_update().clone();
                    tx_cfg.send(AppEvent::Config(cfg)).ok();
                }
            });

            // Producer: poll available_monitors() every second; send when layout changes.
            let app_poll = app.handle().clone();
            let tx_mon = event_tx.clone();
            let initial_monitors_poll = initial_monitors.clone();
            tauri::async_runtime::spawn(async move {
                let monitor_key = |mons: &[tauri::Monitor]| -> Vec<((i32, i32), (u32, u32))> {
                    mons.iter()
                        .map(|m| {
                            let p = m.position();
                            let s = m.size();
                            ((p.x, p.y), (s.width, s.height))
                        })
                        .collect()
                };
                let mut last_key = monitor_key(&initial_monitors_poll);
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let monitors = match app_poll.available_monitors() {
                        Ok(m) if !m.is_empty() => m,
                        Ok(_) => app_poll
                            .primary_monitor()
                            .ok()
                            .flatten()
                            .into_iter()
                            .collect(),
                        Err(_) => continue,
                    };
                    let key = monitor_key(&monitors);
                    if key != last_key {
                        last_key = key;
                        tx_mon.send(AppEvent::Monitors(monitors)).ok();
                    }
                }
            });

            // Producer: poll windows every second; send when list changes.
            let tx_win = event_tx.clone();
            tauri::async_runtime::spawn(async move {
                let mut prev_windows: Vec<window_info::WindowInfo> =
                    vec![window_info::WindowInfo {
                        focused: false,
                        id: 0,
                        rect: LogicalRect {
                            position: LogicalPosition { x: 0.0, y: 0.0 },
                            size: LogicalSize {
                                width: -1.0,
                                height: -1.0,
                            },
                        },
                    }];
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let windows = get_all_windows();
                    if windows != prev_windows {
                        prev_windows = windows.clone();
                        tx_win.send(AppEvent::Windows(windows)).ok();
                    }
                }
            });

            // Orchestrator: react to config or monitor changes by creating any missing windows.
            // Existing windows manage their own updates and destruction via DesktopWindow::run().
            let app_orch = app.handle().clone();
            let mut event_rx = event_tx.subscribe();
            let mut last_config = CONFIG.borrow().clone();
            let mut last_monitors = initial_monitors.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match event_rx.recv().await {
                        Ok(AppEvent::Config(cfg)) => {
                            last_config = cfg;
                            create_missing_windows(
                                &app_orch,
                                &last_config,
                                &last_monitors,
                                event_tx.clone(),
                            );
                        }
                        Ok(AppEvent::Monitors(mons)) => {
                            last_monitors = mons;
                            create_missing_windows(
                                &app_orch,
                                &last_config,
                                &last_monitors,
                                event_tx.clone(),
                            );
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                        _ => {}
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
