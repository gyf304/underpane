use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

use crate::app::APP_HANDLE;
use crate::config::show_config_ui;
use crate::locale::T;

pub fn init() {
    let app = APP_HANDLE.clone();
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
                show_config_ui(app);
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
}
