use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Serialize;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use tauri::Emitter;
use tauri::EventTarget;
use tauri::Manager;
use tauri::{LogicalPosition, LogicalRect, LogicalSize};

use crate::app::APP_HANDLE;
use crate::config::{MonitorConfig, Scalar, CONFIG};
use crate::cursor_position::CURSOR_POSITION;
use crate::monitor_info::MONITORS;
use crate::utils::Tracker;
use crate::wallpapers::{WallpaperConfigSchema, WallpaperManifest};
use crate::window_info::WINDOWS;
use crate::window_info::{coverage, filter_windows};

const RUNTIME_JS: &str = include_str!("runtime.js");

/// Characters to percent-encode within a single URL path segment (the file name
/// of a `file` input). Encodes controls, space, and characters that would
/// otherwise be interpreted as delimiters.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

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

/// Reparents the Tauri webview window so it sits between the desktop wallpaper
/// and the desktop icons.
#[cfg(windows)]
fn set_window_as_background_windows(
    window: &tauri::WebviewWindow,
    monitor: &tauri::Monitor,
) -> Result<(), BackgroundError> {
    use windows::core::{BOOL, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowExW, FindWindowW, GetWindowLongPtrW, SendMessageTimeoutW, SetParent,
        SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM, SMTO_NORMAL,
        SWP_NOACTIVATE, SWP_SHOWWINDOW, WS_CHILD, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
        WS_EX_TOOLWINDOW, WS_VISIBLE,
    };

    let hwnd = window.hwnd()?;

    unsafe {
        let progman = FindWindowW(PCWSTR(wide("Progman").as_ptr()), PCWSTR::null())?;

        // 0x052C asks Progman to spawn a WorkerW behind the desktop icons.
        let mut result: usize = 0;
        SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0xD),
            LPARAM(0x1),
            SMTO_NORMAL,
            1000,
            Some(&mut result as *mut _ as *mut _),
        );

        // Find SHELLDLL_DefView and the WorkerW immediately following its host
        // top-level window in z-order (the wallpaper slot).
        struct Ctx {
            shell_def_view: HWND,
            worker_w: HWND,
        }
        unsafe extern "system" fn enum_proc(top: HWND, lparam: LPARAM) -> BOOL {
            let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
            let p = unsafe {
                FindWindowExW(
                    Some(top),
                    None,
                    PCWSTR(wide("SHELLDLL_DefView").as_ptr()),
                    PCWSTR::null(),
                )
            };
            if let Ok(p) = p {
                if !p.is_invalid() {
                    ctx.shell_def_view = p;
                    if let Ok(w) = unsafe {
                        FindWindowExW(
                            None,
                            Some(top),
                            PCWSTR(wide("WorkerW").as_ptr()),
                            PCWSTR::null(),
                        )
                    } {
                        ctx.worker_w = w;
                    }
                }
            }
            BOOL(1)
        }
        let mut ctx = Ctx {
            shell_def_view: HWND::default(),
            worker_w: HWND::default(),
        };
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));

        // Raised desktop: Progman has WS_EX_NOREDIRECTIONBITMAP; the wallpaper
        // WorkerW is a child of Progman.
        let progman_ex = GetWindowLongPtrW(progman, GWL_EXSTYLE);
        let is_raised = (progman_ex & WS_EX_NOREDIRECTIONBITMAP.0 as isize) != 0;
        if is_raised {
            if let Ok(w) = FindWindowExW(
                Some(progman),
                None,
                PCWSTR(wide("WorkerW").as_ptr()),
                PCWSTR::null(),
            ) {
                ctx.worker_w = w;
            }
        }

        let style = WS_CHILD.0 | WS_VISIBLE.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize,
        );

        let parent = if is_raised { progman } else { ctx.worker_w };
        let _ = SetParent(hwnd, Some(parent));

        // On legacy desktops, SetParent does not move WebView2's DComp visuals,
        // so the wallpaper stays invisible. Calling SetParentWindow here on a
        // raised desktop would hoist Chrome_WidgetWin_0 above SHELLDLL_DefView
        // and obscure the icons.
        if !is_raised {
            let parent_raw = parent.0 as usize;
            let _ = window.with_webview(move |webview| {
                #[cfg(windows)]
                unsafe {
                    let _ = webview
                        .controller()
                        .SetParentWindow(HWND(parent_raw as *mut std::ffi::c_void));
                }
            });
        }

        let pos = monitor.position();
        let size = monitor.size();
        let z = if !ctx.shell_def_view.is_invalid() {
            ctx.shell_def_view
        } else {
            HWND_BOTTOM
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

fn wallpaper_url(index: usize, wallpaper: &str) -> url::Url {
    let i1 = index + 1;
    let mut u = url::Url::parse("underpane-wallpaper://wallpaper").unwrap();
    u.set_host(Some(&format!("monitor-{i1}.{wallpaper}")))
        .unwrap();
    u
}

fn wallpaper_navigate_url(index: usize, wallpaper: &str) -> url::Url {
    #[cfg(windows)]
    {
        let i1 = index + 1;
        // wry's custom-protocol workaround for WebView2: maps custom-scheme://host -> http://custom-scheme.host
        return url::Url::parse(&format!(
            "http://underpane-wallpaper.monitor-{i1}.{wallpaper}"
        ))
        .unwrap();
    }

    #[cfg(not(windows))]
    return wallpaper_url(index, wallpaper);
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
                tauri::WebviewUrl::CustomProtocol(wallpaper_url(index, &monitor_config.wallpaper)),
            )
            .title("underpane")
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

            // Rewrite `file` inputs from their on-disk path to a domain-relative
            // proxy path. The wallpaper page is served from the
            // `underpane-wallpaper://monitor-{n}.{wallpaper}` origin, so this
            // relative path resolves against it and the protocol handler maps it
            // back to the file on disk.
            for (key, schema) in &manifest.config {
                if !matches!(schema, WallpaperConfigSchema::File { .. }) {
                    continue;
                }
                let Some(Scalar::String(path)) = monitor_config.config.get(key) else {
                    continue;
                };
                if path.is_empty() {
                    continue;
                }
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".to_string());
                let encoded = utf8_percent_encode(&filename, PATH_SEGMENT).to_string();
                let proxy = format!("/.underpane/external-file/{key}/{encoded}");
                monitor_config
                    .config
                    .insert(key.clone(), Scalar::String(proxy));
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
        let mut cursor_rx = CURSOR_POSITION.clone();

        let mut tracked_coverage = Tracker::new(0.0);
        let mut tracked_focused = Tracker::new(false);
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
                        let _ = self.window.navigate(wallpaper_navigate_url(self.index, tracked_wallpaper.get()));
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
                Ok(_) = cursor_rx.changed() => {
                    if *tracked_coverage.get() >= 1.0 {
                        continue;
                    }
                    let cursor = *cursor_rx.borrow_and_update();

                    let monitor = MONITORS
                        .borrow()
                        .get(self.index)
                        .ok_or(anyhow::anyhow!("Invalid monitor index"))?
                        .clone();

                    let pos = monitor.position();
                    let sf = monitor.scale_factor();
                    let x = (cursor.x - pos.x as f64) / sf;
                    let y = (cursor.y - pos.y as f64) / sf;

                    let _ = self.emit(
                        "cursor-position",
                        serde_json::json!({ "x": x, "y": y }),
                    );
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
