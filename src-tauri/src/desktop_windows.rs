use serde::Serialize;
use std::sync::Arc;
use tauri::Emitter;
use tauri::EventTarget;
use tauri::Manager;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};

use crate::config::MonitorConfig;
use crate::config::CONFIG;
use crate::monitor_info::MONITORS;
use crate::utils::Tracker;
use crate::window_info::WINDOWS;
use crate::window_info::{coverage, filter_windows};

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

fn logical_monitor_rect(index: usize) -> Option<LogicalRect<f64, f64>> {
    let monitors = MONITORS.borrow();
    let Some(monitor) = monitors.get(index) else {
        return None;
    };
    let work_area_physical = monitor.work_area();
    let scale_factor = monitor.scale_factor();
    Some(LogicalRect {
        position: LogicalPosition {
            x: work_area_physical.position.x as f64 / scale_factor,
            y: work_area_physical.position.y as f64 / scale_factor,
        },
        size: LogicalSize {
            width: work_area_physical.size.width as f64 / scale_factor,
            height: work_area_physical.size.height as f64 / scale_factor,
        },
    })
}

fn calc_visibility(index: usize) -> Option<(f64, bool)> {
    let Some(rect) = logical_monitor_rect(index) else {
        return None;
    };
    let visible_windows = filter_windows(&WINDOWS.borrow(), &rect).clone();
    let focused = (&visible_windows).into_iter().filter(|w| w.focused).count() == 0;
    let cov = coverage(&visible_windows, &rect);
    Some((cov, focused))
}

/// Manages a single desktop window for one monitor index.
#[derive(Clone)]
pub struct DesktopWindow {
    /// 0-based monitor index.
    index: usize,
    window: Arc<tauri::WebviewWindow>,
    handle: Option<Arc<tauri::async_runtime::JoinHandle<()>>>,
}

impl DesktopWindow {
    pub fn new(app: &tauri::AppHandle, index: usize) -> Result<Self, tauri::Error> {
        let monitor = MONITORS
            .borrow()
            .get(index)
            .ok_or(anyhow::anyhow!("Invalid monitor index"))?
            .clone();

        let i1 = index + 1;
        let label = format!("monitor-{i1}");
        let monitor_config = CONFIG
            .borrow()
            .get_monitor_config(index)
            .ok_or(anyhow::anyhow!("Invalid config index"))?
            .clone();
        let monitor_clone = monitor.clone();

        let mut init_lines: Vec<String> = vec![];
        if let Ok(config) = serde_json::to_string(&monitor_config.config) {
            init_lines.push(format!("config = {config};"));
        }
        if let Some((cov, focused)) = calc_visibility(index) {
            init_lines.push(format!("coverage = {cov};"));
            init_lines.push(format!("focused = {focused};"));
        }
        let init_str = init_lines.join(";\n");

        let window = Arc::new(
            tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::CustomProtocol(wallpaper_url(&monitor_config.wallpaper)),
            )
            .title("activedesk")
            .transparent(true)
            .decorations(false)
            .focused(false)
            .skip_taskbar(true)
            .resizable(false)
            .hidden_title(true)
            .shadow(false)
            .initialization_script(&format!("(function () {{
                {RUNTIME_JS};
                {init_str};
            }})();"))
            .build()?,
        );
        let window_clone = window.clone();

        app.run_on_main_thread(move || {
            set_window_as_background(window_clone.as_ref(), &monitor_clone).ok();
        })?;

        let mut desktop_window = DesktopWindow {
            index,
            window,
            handle: None,
        };
        let desktop_window_clone = desktop_window.clone();

        let handle = tauri::async_runtime::spawn(async move {
            let _ = desktop_window_clone.run_window_async().await;
        });
        desktop_window.handle = Some(Arc::new(handle));
        let _ = desktop_window.resize_window(&monitor);

        Ok(desktop_window)
    }

    pub fn monitor(&self) -> Option<tauri::Monitor> {
        MONITORS.borrow().get(self.index).cloned()
    }

    pub fn monitor_config(&self) -> Option<MonitorConfig> {
        CONFIG.borrow().get_monitor_config(self.index).cloned()
    }

    pub fn emit<S>(&self, event: &str, payload: S) -> Result<(), tauri::Error> where S: Serialize + Clone {
        let label = self.window.label().to_string();
        self.window
            .app_handle()
            .emit_to(
                EventTarget::WebviewWindow { label },
                event,
                payload,
            )
    }

    async fn run_window_async(&self) -> Result<(), tauri::Error> {
        let mut config_rx = CONFIG.clone();
        let mut monitors_rx = MONITORS.clone();
        let mut windows_rx = WINDOWS.clone();

        let mut cursor_tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 30.0));

        let mut tracked_coverage = Tracker::new(0.0);
        let mut tracked_focused = Tracker::new(false);
        let mut tracked_cursor_position = Tracker::new((0.0, 0.0));

        loop {
            tokio::select! {
                Ok(_) = config_rx.changed() => {
                    let Some(monitor_config) = self.monitor_config() else {
                        continue
                    };
                    let _ = self.emit(
                        "config-change",
                        serde_json::json!({ "config": monitor_config.config }),
                    );
                }
                Ok(_) = monitors_rx.changed() => {
                    let Some(monitor) = self.monitor() else {
                        continue
                    };
                    let _ = self.resize_window(&monitor);
                }
                Ok(_) = windows_rx.changed() => {
                    let Some((cov, focused)) = calc_visibility(self.index) else {
                        continue
                    };
                    if tracked_coverage.update(cov) {
                        let _ = self.emit(
                            "desktop-coverage",
                            serde_json::json!({ "coverage": tracked_coverage.get() }),
                        );
                    }
                    if tracked_focused.update(focused) {
                        let _ = self.emit(
                            "desktop-focus",
                            serde_json::json!({ "focused": tracked_focused.get() }),
                        );
                    }
                }
                _ = cursor_tick.tick() => {
                    let Ok(cursor) = self.window.app_handle().cursor_position() else {
                        continue;
                    };

                    let monitor = MONITORS
                        .borrow()
                        .get(self.index)
                        .ok_or(anyhow::anyhow!("Invalid monitor index"))?
                        .clone();

                    let pos = monitor.position();
                    let sf = monitor.scale_factor();
                    let x = (cursor.x - pos.x as f64) / sf;
                    let y = (cursor.y - pos.y as f64) / sf;

                    let cursor_position = (x, y);
                    if tracked_cursor_position.update(cursor_position) {
                        let _ = self.emit(
                            "cursor-position",
                            serde_json::json!({ "x": x, "y": y }),
                        );
                    }
                }
            }
        }
    }

    fn resize_window(&self, monitor: &tauri::Monitor) -> Result<(), tauri::Error> {
        let window = self.window.clone();
        let position = monitor.position().clone();
        let size = monitor.size().clone();
        window.clone().run_on_main_thread(move || {
            let _ = window.set_position(position);
            let _ = window.set_size(size);
        })?;
        Ok(())
    }
}

impl Drop for DesktopWindow {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
        let _ = self.window.close();
    }
}
