use tauri::Emitter;
use tauri::EventTarget;
use tauri::Manager;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};
use tokio::sync::broadcast;

use crate::config;
use crate::window_info::{WindowInfo, filter_windows, coverage};


#[derive(Clone, Debug)]
pub enum AppEvent {
    Config(config::Config),
    Monitors(Vec<tauri::Monitor>),
    Windows(Vec<WindowInfo>),
}

const RUNTIME_JS: &str = include_str!("runtime.js");

#[derive(Debug)]
pub enum BackgroundError {
    NotMainThread,
    Tauri(tauri::Error),
}

impl std::fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackgroundError::NotMainThread => write!(f, "must be called on the main thread"),
            BackgroundError::Tauri(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BackgroundError {}

impl From<tauri::Error> for BackgroundError {
    fn from(e: tauri::Error) -> Self {
        BackgroundError::Tauri(e)
    }
}

pub fn set_window_as_background(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
) -> Result<(), BackgroundError> {
    #[cfg(target_os = "macos")]
    set_window_as_background_macos(window, monitor)?;
    Ok(())
}

/// Sets the given window as a background window on macOS, stretching it to
/// cover the given monitor.
///
/// The window level is set to `kCGDesktopWindowLevel + 1`, which places it
/// above the wallpaper but below Finder icons and all application windows.
#[cfg(target_os = "macos")]
fn set_window_as_background_macos(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
) -> Result<(), BackgroundError> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::NSApplication;
    use objc2_app_kit::NSApplicationActivationPolicy;
    use objc2_app_kit::NSWindowCollectionBehavior;
    use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new().ok_or(BackgroundError::NotMainThread)?;
    let ns_app = NSApplication::sharedApplication(mtm);

    // Stretch the window to cover the given monitor.
    // We use Tauri's monitor API rather than `fullscreen: true` so the window
    // stays in the normal window level hierarchy and doesn't enter macOS
    // fullscreen mode (which would move it to its own Space).
    window.set_size(*monitor.size())?;
    window.set_position(*monitor.position())?;

    // Drop the window to just above the desktop wallpaper layer.
    let ptr = window.ns_window()?;

    unsafe {
        let ns_window = ptr as *mut AnyObject;

        let behavior = NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::IgnoresCycle;

        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];

        // kCGDesktopWindowLevelKey = 3 per <CoreGraphics/CGWindowLevel.h>.
        // +1 puts us above the raw wallpaper layer but still below Finder icons
        // (kCGDesktopIconWindowLevel = kCGDesktopWindowLevel + 20) and every
        // application or system window.
        let level = CGWindowLevelForKey(CGWindowLevelKey::DesktopWindowLevelKey) + 1;

        // NSWindowLevel / NSInteger is isize on 64-bit macOS.
        let _: () = msg_send![ns_window, setLevel: level as isize];
        let _: () = msg_send![ns_window, setStyleMask: 0usize];

        let _: () =
            msg_send![&*ns_app, setActivationPolicy: NSApplicationActivationPolicy::Accessory];
    }

    Ok(())
}

fn wallpaper_url(wallpaper: &str) -> url::Url {
    let mut u = url::Url::parse("activedesk-wallpaper://wallpaper").unwrap();
    u.set_host(Some(wallpaper)).unwrap();
    u
}

/// Manages a single desktop window for one monitor index.
///
/// Holds a broadcast receiver for [`AppEvent`] so it can react to config and
/// monitor changes independently without a global scan.
pub struct DesktopWindow {
    /// 0-based monitor index.
    index: usize,
    window: tauri::WebviewWindow,
    event_rx: broadcast::Receiver<AppEvent>,
    monitor: tauri::Monitor,
    /// Whether the desktop is currently considered focused (no regular app window has focus).
    focused: bool,
}

impl DesktopWindow {
    /// Drive this window until its monitor or config entry disappears,
    /// or until the event sender is dropped.
    pub async fn run(mut self) {
        let mut prev_wallpaper_config = toml::Table::new();
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 30.0));
        ticker.tick().await; // consume the instant-fire first tick

        loop {
            tokio::select! {
                res = self.event_rx.recv() => {
                    match res {
                        Ok(AppEvent::Config(cfg)) => {
                            let i1 = self.index + 1;
                            let Some(wp) = cfg.monitors.get(&i1.to_string()) else {
                                // Config entry removed — close and exit.
                                let win = self.window.clone();
                                win.clone().run_on_main_thread(move || { win.close().ok(); }).ok();
                                return;
                            };
                            let url = wallpaper_url(&wp.wallpaper);
                            if self.window.url().map(|u| u != url).unwrap_or(false) {
                                self.window.navigate(url).ok();
                            }
                            let wallpaper_config = wp.config.clone();
                            if wallpaper_config != prev_wallpaper_config {
                                println!("config change");
                                prev_wallpaper_config = wallpaper_config.clone();
                                self.window.app_handle().emit_to(
                                    EventTarget::WebviewWindow { label: self.window.label().to_string() },
                                    "wallpaper-config",
                                    wallpaper_config,
                                ).ok();
                            }
                        }
                        Ok(AppEvent::Monitors(monitors)) => {
                            let Some(m) = monitors.get(self.index) else {
                                // Monitor removed — close and exit.
                                let win = self.window.clone();
                                win.clone().run_on_main_thread(move || { win.close().ok(); }).ok();
                                return;
                            };
                            let (size, pos) = (*m.size(), *m.position());
                            self.monitor = m.clone();
                            let win = self.window.clone();
                            win.clone().run_on_main_thread(move || {
                                win.set_size(size).ok();
                                win.set_position(pos).ok();
                            }).ok();
                        }
                        Ok(AppEvent::Windows(windows)) => {
                            // Filter windows to this monitor's bounds and calculate coverage
                            let work_area_physical = self.monitor.work_area();
                            let scale_factor = self.monitor.scale_factor();
                            let work_area = LogicalRect {
                                position: LogicalPosition {
                                    x: work_area_physical.position.x as f64 / scale_factor,
                                    y: work_area_physical.position.y as f64 / scale_factor,
                                },
                                size: LogicalSize {
                                    width: work_area_physical.size.width as f64 / scale_factor,
                                    height: work_area_physical.size.height as f64 / scale_factor,
                                }
                            };
                            let visible_windows = filter_windows(&windows, &work_area);

                            let cov = coverage(&windows, &work_area);

                            let focused = visible_windows.into_iter()
                                .filter(|w| w.focused)
                                .count() == 0;

                            let label = self.window.label().to_string();

                            // Emit coverage event to JS
                            self.window
                                .app_handle()
                                .emit_to(
                                    EventTarget::WebviewWindow { label: label.clone() },
                                    "desktop-coverage",
                                    serde_json::json!({ "coverage": cov }),
                                )
                                .ok();

                            // Check focus state and emit events only when changed
                            if focused != self.focused {
                                self.focused = focused;
                                self.window.app_handle().emit_to(
                                    EventTarget::WebviewWindow { label: label.clone() },
                                    "desktop-focus",
                                    serde_json::json!({ "focused": focused }),
                                ).ok();
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = ticker.tick() => {
                    let Ok(cursor) = self.window.app_handle().cursor_position() else {
                        continue;
                    };
                    let pos = self.monitor.position();
                    let sf = self.monitor.scale_factor();
                    let x = (cursor.x - pos.x as f64) / sf;
                    let y = (cursor.y - pos.y as f64) / sf;
                    let label = self.window.label().to_string();

                    self.window
                        .app_handle()
                        .emit_to(
                            EventTarget::WebviewWindow { label },
                            "cursor-position",
                            serde_json::json!({ "x": x, "y": y }),
                        )
                        .ok();
                }
            }
        }
    }
}

/// Create a desktop window for the given monitor index and spawn its [`DesktopWindow::run`] task.
///
/// Window construction and [`set_window_as_background`] must run on the main thread, so this
/// function dispatches there internally. It is a no-op if a window with the same label already
/// exists.
pub fn spawn_window(
    app: &tauri::AppHandle,
    index: usize,
    monitor: tauri::Monitor,
    wallpaper: config::WallpaperConfig,
    event_tx: broadcast::Sender<AppEvent>,
) {
    let app = app.clone();
    app.clone().run_on_main_thread(move || {
        let i1 = index + 1;
        let label = format!("monitor-{i1}");

        if app.webview_windows().contains_key(&label) {
            return;
        }

        let _json = serde_json::to_string(&wallpaper.config).unwrap_or_default();
        let builder = tauri::WebviewWindowBuilder::new(
            &app,
            &label,
            tauri::WebviewUrl::CustomProtocol(wallpaper_url(&wallpaper.wallpaper)),
        )
        .title("activedesk")
        .transparent(true)
        .decorations(false)
        .focused(false)
        .skip_taskbar(true)
        .resizable(false)
        .hidden_title(true)
        .shadow(false)
        .initialization_script(&format!("(function () {{ {RUNTIME_JS} }})();"));

        match builder.build() {
            Ok(win) => {
                if let Err(e) = set_window_as_background(&win, &monitor) {
                    eprintln!("activedesk: set_window_as_background failed for {label}: {e}");
                }
                let event_rx = event_tx.subscribe();
                tauri::async_runtime::spawn(
                    DesktopWindow { index, window: win, event_rx, monitor, focused: false }.run(),
                );
            }
            Err(e) => eprintln!("activedesk: failed to create window {label}: {e}"),
        }
    }).ok();
}
