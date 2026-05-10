use serde::Serialize;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use tauri::Emitter;
use tauri::EventTarget;
use tauri::Manager;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};

use crate::app::APP_HANDLE;
use crate::config::{MonitorConfig, CONFIG};
use crate::monitor_info::MONITORS;
use crate::utils::Tracker;
use crate::wallpapers::WallpaperManifest;
use crate::window_info::WINDOWS;
use crate::window_info::{coverage, filter_windows};

const RUNTIME_JS: &str = include_str!("runtime.js");

pub static DESKTOP_WINDOWS: LazyLock<Mutex<Vec<Option<DesktopWindow>>>> =
    LazyLock::new(|| Mutex::new(vec![]));

#[derive(Debug)]
pub enum BackgroundError {
    NotMainThread,
    Tauri(tauri::Error),
    #[cfg(windows)]
    Win32(windows::core::Error),
}

impl std::fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackgroundError::NotMainThread => write!(f, "must be called on the main thread"),
            BackgroundError::Tauri(e) => write!(f, "{e}"),
            #[cfg(windows)]
            BackgroundError::Win32(e) => write!(f, "win32: {e}"),
        }
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for BackgroundError {
    fn from(e: windows::core::Error) -> Self {
        BackgroundError::Win32(e)
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
    #[cfg(windows)]
    set_window_as_background_windows(window, monitor)?;
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

/// Reparents the given Tauri webview window into a `WorkerW` sibling of
/// Progman so it sits between the desktop wallpaper and the desktop icons.
///
/// Uses the well-known Progman `0x052C` message trick (as employed by
/// Lively Wallpaper / Wallpaper Engine) to spawn the WorkerW.
#[cfg(windows)]
fn set_window_as_background_windows(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
) -> Result<(), BackgroundError> {
    use windows::core::{BOOL, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetWindowLongPtrW,
        SendMessageTimeoutW, SetParent, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
        HWND_BOTTOM, HWND_TOP, SMTO_NORMAL, SWP_NOACTIVATE, SWP_SHOWWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
    };

    let hwnd = window.hwnd()?;

    unsafe {
        // 1. Find Progman.
        let progman = FindWindowW(PCWSTR(wide("Progman").as_ptr()), PCWSTR::null())?;

        // 2. Tell Progman to spawn a WorkerW behind the desktop icons.
        let mut result: usize = 0;
        SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            Some(&mut result as *mut _ as *mut _),
        );

        // 3. Find whichever top-level window hosts the desktop icon layer
        //    (SHELLDLL_DefView child). On some Windows versions this is a
        //    WorkerW; on Windows 11 it's often Progman itself.
        struct Ctx {
            icon_host: HWND,
        }
        unsafe extern "system" fn enum_proc(top: HWND, lparam: LPARAM) -> BOOL {
            let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
            let shell_view = unsafe {
                FindWindowExW(
                    Some(top),
                    None,
                    PCWSTR(wide("SHELLDLL_DefView").as_ptr()),
                    PCWSTR::null(),
                )
            };
            if shell_view.is_ok() && !shell_view.unwrap().is_invalid() {
                ctx.icon_host = top;
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut ctx = Ctx {
            icon_host: HWND::default(),
        };
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));

        // 4. Choose parent:
        //    - If the icon host is a WorkerW, find the *other* WorkerW that
        //      lives behind it (the classic Wallpaper Engine slot).
        //    - If the icon host is Progman (Windows 11 common case), parent
        //      under Progman directly and push to HWND_BOTTOM so we sit
        //      behind SHELLDLL_DefView. Progman is always at HWND_BOTTOM in
        //      the top-level Z-order, so everything else stays above us.
        let icon_host_class = {
            let mut buf = [0u16; 64];
            let n = GetClassNameW(ctx.icon_host, &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        };

        let (parent, needs_bottom) = if icon_host_class == "WorkerW" {
            // Find a WorkerW that is NOT the icon host.
            let worker_class = wide("WorkerW");
            let mut behind = HWND::default();
            let mut cur = FindWindowExW(None, None, PCWSTR(worker_class.as_ptr()), PCWSTR::null());
            while let Ok(w) = cur {
                if w.is_invalid() {
                    break;
                }
                if w != ctx.icon_host {
                    behind = w;
                    break;
                }
                cur = FindWindowExW(None, Some(w), PCWSTR(worker_class.as_ptr()), PCWSTR::null());
            }
            let p = if !behind.is_invalid() {
                behind
            } else {
                progman
            };
            (p, false)
        } else {
            // icon_host is Progman: parent under it and sit behind its children.
            (progman, true)
        };

        // 5. Strip decorations & taskbar visibility, mark non-activating.
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP.0 | WS_VISIBLE.0) as isize);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize,
        );

        // 6. Reparent.
        let _ = SetParent(hwnd, Some(parent));

        // 7. Position to cover the monitor in physical pixels.
        //    When parented under Progman, insert just behind SHELLDLL_DefView
        //    so icons stay above us but we're in front of any deeper children
        //    (e.g. the WorkerW child) that would otherwise occlude us.
        let pos = monitor.position();
        let size = monitor.size();
        let shell_view = FindWindowExW(
            Some(parent),
            None,
            PCWSTR(wide("SHELLDLL_DefView").as_ptr()),
            PCWSTR::null(),
        )
        .unwrap_or_default();
        let z = if needs_bottom && !shell_view.is_invalid() {
            shell_view // just behind the icon layer
        } else if needs_bottom {
            HWND_BOTTOM
        } else {
            HWND_TOP
        };
        let _ = SetWindowPos(
            hwnd,
            Some(z),
            pos.x,
            pos.y,
            size.width as i32,
            size.height as i32,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );

        let _ = result;
    }

    Ok(())
}

/// Encodes a string as a NUL-terminated UTF-16 buffer for Win32 wide APIs.
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn wallpaper_url(wallpaper: &str) -> url::Url {
    let mut u = url::Url::parse("activedesk-wallpaper://wallpaper").unwrap();
    u.set_host(Some(wallpaper)).unwrap();
    u
}

fn wallpaper_navigate_url(wallpaper: &str) -> url::Url {
    #[cfg(windows)]
    // wry's custom-protocol workaround for WebView2: maps custom-scheme://host → http://custom-scheme.host
    return url::Url::parse(&format!("http://activedesk-wallpaper.{wallpaper}")).unwrap();

    #[cfg(not(windows))]
    return wallpaper_url(wallpaper);
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

pub(crate) fn calc_desktop_visibility(index: usize) -> Option<(f64, bool)> {
    let Some(rect) = logical_monitor_rect(index) else {
        return None;
    };
    let visible_windows = filter_windows(&WINDOWS.borrow(), &rect).clone();
    let desktop_has_focus = visible_windows.iter().filter(|w| w.focused).count() == 0;
    let cov = coverage(&visible_windows, &rect);
    Some((cov, desktop_has_focus))
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
            .shadow(false)
            .initialization_script(&format!(
                "(async function () {{
                {RUNTIME_JS};
            }})();"
            ))
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
        let mut monitor_config = CONFIG.borrow().get_monitor_config(self.index).cloned()?;

        if let Ok(manifest) = WallpaperManifest::get(&monitor_config.wallpaper) {
            for (key, value) in manifest.default_config() {
                monitor_config.config.entry(key).or_insert(value);
            }
        }

        Some(monitor_config)
    }

    pub fn emit<S>(&self, event: &str, payload: S) -> Result<(), tauri::Error>
    where
        S: Serialize + Clone,
    {
        let label = self.window.label().to_string();
        self.window
            .app_handle()
            .emit_to(EventTarget::WebviewWindow { label }, event, payload)
    }

    async fn run_window_async(&self) -> Result<(), tauri::Error> {
        let mut config_rx = CONFIG.clone();
        let mut monitors_rx = MONITORS.clone();
        let mut windows_rx = WINDOWS.clone();

        let mut cursor_tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 30.0));

        let mut tracked_coverage = Tracker::new(0.0);
        let mut tracked_focused = Tracker::new(false);
        let mut tracked_cursor_position = Tracker::new((0.0, 0.0));
        let mut tracked_wallpaper = Tracker::new(
            self.monitor_config()
                .map(|c| c.wallpaper)
                .unwrap_or_default(),
        );

        loop {
            tokio::select! {
                Ok(_) = config_rx.changed() => {
                    let Some(monitor_config) = self.monitor_config() else {
                        continue
                    };
                    if tracked_wallpaper.update(monitor_config.wallpaper.clone()) {
                        let _ = self.window.navigate(wallpaper_navigate_url(tracked_wallpaper.get()));
                    }
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
                    let Some((cov, focused)) = calc_desktop_visibility(self.index) else {
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

pub fn sync_desktop_windows() -> Result<(), tauri::Error> {
    let app = &APP_HANDLE;
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
