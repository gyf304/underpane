use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tauri::{AppHandle, Listener};

use crate::locale::T;

pub fn init(app_handle: &AppHandle) {
    let app = app_handle.clone();
    app_handle.once("locales-configured", move |_| {
        let app = &app;
        let configure = MenuItem::with_id(
            app,
            "configure",
            T.get("tray.configure"),
            true,
            None::<&str>,
        )
        .unwrap();
        let refresh_wallpapers = MenuItem::with_id(
            app,
            "refresh_wallpapers",
            T.get("tray.refresh_wallpapers"),
            true,
            None::<&str>,
        )
        .unwrap();
        let quit = MenuItem::with_id(app, "quit", T.get("tray.quit"), true, None::<&str>).unwrap();
        let menu = Menu::with_items(app, &[&configure, &refresh_wallpapers, &quit]).unwrap();

        let _ = TrayIconBuilder::new()
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
                            tauri::WebviewUrl::App(std::path::PathBuf::from("index.html")),
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
                "quit" => app.exit(0),
                _ => {}
            })
            .build(app);
    });
}
