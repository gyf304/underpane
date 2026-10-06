use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use tauri::Emitter;
use tauri::EventTarget;
use tauri::Manager;

use crate::app::APP_HANDLE;
use crate::config::{MonitorConfig, Scalar, CONFIG};
use crate::monitor_info::MONITORS;
use crate::utils::Tracker;
use crate::wallpapers::{WallpaperConfigSchema, WallpaperManifest};
use crate::window_daemon::WindowDaemon;

const RUNTIME_JS: &str = include_str!("runtime.js");

/// Characters to percent-encode within a single URL path segment (the file name
/// of a `file` input). Encodes controls, space, and characters that would
/// otherwise be interpreted as delimiters.
pub(crate) const PATH_SEGMENT: &AsciiSet = &CONTROLS
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

/// Builds the navigate-form origin for one realm of a monitor's wallpaper. Each
/// realm gets its own origin so the wallpaper's own bundled files (`wallpaper`)
/// stay isolated from arbitrary user-selected files (`asset`). On Windows this
/// is wry's `http://underpane.<host>` WebView2 workaround form; elsewhere it's
/// the native `underpane://<host>` custom-scheme form.
pub(crate) fn realm_origin(realm: &str, index: usize, id: &str) -> String {
    let i1 = index + 1;
    #[cfg(windows)]
    {
        // wry's custom-protocol workaround for WebView2: maps custom-scheme://host -> http://custom-scheme.host
        return format!("http://underpane.monitor-{i1}.{id}.{realm}");
    }

    #[cfg(not(windows))]
    return format!("underpane://monitor-{i1}.{id}.{realm}");
}

fn wallpaper_url(index: usize, id: &str) -> url::Url {
    let i1 = index + 1;
    let mut u = url::Url::parse("underpane://wallpaper").unwrap();
    u.set_host(Some(&format!("monitor-{i1}.{id}.wallpaper")))
        .unwrap();
    u
}

fn wallpaper_navigate_url(index: usize, id: &str) -> url::Url {
    #[cfg(windows)]
    {
        return url::Url::parse(&realm_origin("wallpaper", index, id)).unwrap();
    }

    #[cfg(not(windows))]
    return wallpaper_url(index, id);
}

/// The per-monitor config with manifest defaults merged and `file`/`directory`
/// inputs rewritten to their `asset` realm URLs.
pub(crate) fn monitor_config(index: usize) -> Option<MonitorConfig> {
    let mut monitor_config = CONFIG.borrow().get_monitor_config(index).cloned()?;

    if let Ok(manifest) = WallpaperManifest::get(&monitor_config.wallpaper) {
        for (key, value) in manifest.default_config() {
            monitor_config.config.entry(key).or_insert(value);
        }

        // Rewrite `file`/`directory` inputs from their on-disk path to an
        // absolute URL in the wallpaper's `asset` realm origin. Serving
        // user-selected files from a separate origin than the wallpaper's own
        // code keeps the two isolated; the protocol handler maps the URL back
        // to disk. A `file` resolves to one file; a `directory` becomes a base
        // URL (trailing slash) the page appends relative paths to.
        for (key, schema) in &manifest.config {
            let is_dir = match schema {
                WallpaperConfigSchema::File { .. } => false,
                WallpaperConfigSchema::Directory { .. } => true,
                _ => continue,
            };
            let Some(Scalar::String(path)) = monitor_config.config.get(key) else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            let origin = realm_origin("asset", index, &monitor_config.wallpaper);
            let encoded_key = utf8_percent_encode(key, PATH_SEGMENT).to_string();
            let url = if is_dir {
                format!("{origin}/{encoded_key}/")
            } else {
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".to_string());
                let encoded_filename = utf8_percent_encode(&filename, PATH_SEGMENT).to_string();
                format!("{origin}/{encoded_key}/{encoded_filename}")
            };
            monitor_config
                .config
                .insert(key.clone(), Scalar::String(url));
        }
    }

    Some(monitor_config)
}

/// Manages a single desktop window for one monitor index.
pub struct DesktopWindow {
    /// 0-based monitor index.
    index: usize,
    window: Arc<tauri::WebviewWindow>,
    daemon: Option<WindowDaemon>,
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

        let daemon = WindowDaemon::new(&window, &monitor, index)?;
        let handle = tauri::async_runtime::spawn(run_config_watcher(window.clone(), index));

        Ok(DesktopWindow {
            index,
            window,
            daemon: Some(daemon),
            handle: Some(Arc::new(handle)),
        })
    }

    pub fn monitor_config(&self) -> Option<MonitorConfig> {
        monitor_config(self.index)
    }
}

/// Navigates the window when its configured wallpaper changes and forwards new
/// config to the page.
async fn run_config_watcher(window: Arc<tauri::WebviewWindow>, index: usize) {
    let mut config_rx = CONFIG.clone();
    let mut tracked_wallpaper = Tracker::new(
        monitor_config(index)
            .map(|config| config.wallpaper)
            .unwrap_or_default(),
    );

    loop {
        if config_rx.changed().await.is_err() {
            break;
        }
        let Some(config) = monitor_config(index) else {
            continue;
        };
        if tracked_wallpaper.update(config.wallpaper.clone()) {
            let _ = window.navigate(wallpaper_navigate_url(index, tracked_wallpaper.get()));
        }
        let target = EventTarget::WebviewWindow {
            label: window.label().to_string(),
        };
        let _ = window.app_handle().emit_to(
            target,
            "config-change",
            serde_json::json!({ "config": config.config }),
        );
    }
}

impl Drop for DesktopWindow {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
        // Stop the daemon (and its OS monitor registration) before the window.
        self.daemon.take();
        let _ = self.window.close();
    }
}

pub fn sync_desktop_windows() -> Result<(), tauri::Error> {
    let app = &APP_HANDLE;
    let mut windows = DESKTOP_WINDOWS.lock().unwrap();
    let monitor_count = MONITORS.borrow().len();
    let config = CONFIG.borrow().clone();

    windows.resize_with(monitor_count, || None);

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
