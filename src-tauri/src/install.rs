use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{is_valid_wallpaper_id, CONFIG};
use crate::wallpapers::WallpaperManifest;

const MAX_COMPRESSED_BYTES: u64 = 100 * 1024 * 1024;
const EMIT_BYTES_THRESHOLD: u64 = 256 * 1024;
const EMIT_MS_THRESHOLD: u128 = 100;

/// Where a wallpaper zip comes from, resolved from the `zip_url` scheme.
enum ZipSource {
    /// `https://…` — streamed and downloaded by ripunzip.
    Remote(String),
    /// `file://…` — an existing local file read directly.
    Local(PathBuf),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
pub enum InstallProgress {
    Progress {
        install_id: String,
        bytes_done: u64,
        bytes_total: Option<u64>,
        files_done: u64,
        files_total: Option<u64>,
    },
    Validate {
        install_id: String,
    },
    Done {
        install_id: String,
    },
    Error {
        install_id: String,
        message: String,
    },
}

fn emit_progress(app: &AppHandle, payload: &InstallProgress) {
    let _ = app.emit_to("config", "install-progress", payload);
}

fn target_wallpaper_dir(app: &AppHandle) -> Result<PathBuf, String> {
    for dir in CONFIG.borrow().get_wallpaper_dirs() {
        // Use the first dir we can create / write to.
        if fs::create_dir_all(&dir).is_ok() {
            return Ok(dir);
        }
    }
    app.path()
        .app_data_dir()
        .map(|p| p.join("wallpapers"))
        .map_err(|e| e.to_string())
}

fn cleanup(p: &Path) {
    let _ = fs::remove_dir_all(p);
}

/// Determine wallpaper root inside `extracted/`. Either `extracted/index.toml`
/// exists, or exactly one top-level subdirectory containing `index.toml`.
fn determine_root(extracted: &Path) -> Result<PathBuf, String> {
    if extracted.join("index.toml").is_file() {
        return Ok(extracted.to_path_buf());
    }
    let mut subdir: Option<PathBuf> = None;
    let entries = fs::read_dir(extracted).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(f) => f,
            Err(_) => continue,
        };
        if !ft.is_dir() {
            continue;
        }
        if subdir.is_some() {
            return Err("zip has multiple top-level entries and no index.toml at root".into());
        }
        subdir = Some(entry.path());
    }
    let sub = subdir.ok_or_else(|| "zip is empty or has no index.toml".to_string())?;
    if !sub.join("index.toml").is_file() {
        return Err("zip's top-level directory does not contain index.toml".into());
    }
    Ok(sub)
}

fn move_dir(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if dst.exists() {
        fs::remove_dir_all(dst).map_err(|e| e.to_string())?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-device or otherwise unsupported — fall back to copy + remove.
            copy_dir_recursive(src, dst)?;
            fs::remove_dir_all(src).map_err(|e| e.to_string())
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ft.is_file() {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct ProgressState {
    last_emit: Option<Instant>,
    last_bytes: u64,
}

struct Reporter {
    app: AppHandle,
    install_id: String,
    bytes_done: AtomicU64,
    bytes_total: Mutex<Option<u64>>,
    files_done: AtomicU64,
    files_total: Mutex<Option<u64>>,
    state: Mutex<ProgressState>,
}

impl Reporter {
    fn new(app: AppHandle, install_id: String) -> Self {
        Self {
            app,
            install_id,
            bytes_done: AtomicU64::new(0),
            bytes_total: Mutex::new(None),
            files_done: AtomicU64::new(0),
            files_total: Mutex::new(None),
            state: Mutex::new(ProgressState::default()),
        }
    }

    fn emit(&self, force: bool) {
        let bytes_done = self.bytes_done.load(Ordering::Relaxed);
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let should_emit = force
            || match state.last_emit {
                None => true,
                Some(t) => {
                    bytes_done.saturating_sub(state.last_bytes) >= EMIT_BYTES_THRESHOLD
                        || now.duration_since(t).as_millis() >= EMIT_MS_THRESHOLD
                }
            };
        if !should_emit {
            return;
        }
        state.last_emit = Some(now);
        state.last_bytes = bytes_done;
        drop(state);
        let payload = InstallProgress::Progress {
            install_id: self.install_id.clone(),
            bytes_done,
            bytes_total: *self.bytes_total.lock().unwrap(),
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: *self.files_total.lock().unwrap(),
        };
        emit_progress(&self.app, &payload);
    }
}

impl ripunzip::UnzipProgressReporter for Reporter {
    fn total_bytes_expected(&self, expected: u64) {
        *self.bytes_total.lock().unwrap() = Some(expected);
        self.emit(true);
    }

    fn bytes_extracted(&self, count: u64) {
        self.bytes_done.fetch_add(count, Ordering::Relaxed);
        self.emit(false);
    }

    fn extraction_finished(&self, _display_name: &str) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
        self.emit(false);
    }
}

/// Download (or read a local `file://` zip), extract, validate, and move a
/// wallpaper bundle into the user's wallpaper directory under `name`. `zip_url`
/// is either an `https://` or a `file://` URL. Emits `install-progress` events
/// scoped to `install_id` throughout.
pub async fn install_wallpaper(
    app: AppHandle,
    name: String,
    zip_url: String,
    install_id: String,
) -> Result<(), String> {
    let result = install_inner(app.clone(), name, zip_url, install_id.clone()).await;
    match &result {
        Ok(()) => emit_progress(
            &app,
            &InstallProgress::Done {
                install_id: install_id.clone(),
            },
        ),
        Err(msg) => emit_progress(
            &app,
            &InstallProgress::Error {
                install_id: install_id.clone(),
                message: msg.clone(),
            },
        ),
    }
    result
}

async fn install_inner(
    app: AppHandle,
    name: String,
    zip_url: String,
    install_id: String,
) -> Result<(), String> {
    if !is_valid_wallpaper_id(&name) {
        return Err(format!(
            "invalid wallpaper name '{name}' (allowed: lowercase letters, digits, '-'; cannot start or end with '-')"
        ));
    }
    // Resolve the source up front so a bad URL or missing file fails fast.
    let url = url::Url::parse(&zip_url).map_err(|e| format!("invalid zip_url: {e}"))?;
    let source = match url.scheme() {
        "https" => ZipSource::Remote(zip_url),
        "file" => {
            let path = url
                .to_file_path()
                .map_err(|()| format!("zip_url is not a local file url: {zip_url}"))?;
            if !path.is_file() {
                return Err(format!("file not found: {}", path.display()));
            }
            ZipSource::Local(path)
        }
        other => return Err(format!("unsupported zip_url scheme '{other}'")),
    };

    let temp_root = app.path().temp_dir().map_err(|e| e.to_string())?;
    let work_dir = temp_root.join(format!("underpane-install-{install_id}"));
    let extracted = work_dir.join("extracted");
    fs::create_dir_all(&extracted).map_err(|e| e.to_string())?;

    // Phase 1: download/open + extract (ripunzip is sync; run on blocking pool).
    let reporter = Arc::new(Reporter::new(app.clone(), install_id.clone()));
    let extracted_for_blocking = extracted.clone();
    let reporter_for_blocking = reporter.clone();

    let unzip_result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let engine = match source {
            ZipSource::Remote(url) => ripunzip::UnzipEngine::for_uri(&url, None, || {})
                .map_err(|e| format!("download init failed: {e}"))?,
            ZipSource::Local(path) => {
                let file = fs::File::open(&path).map_err(|e| format!("open failed: {e}"))?;
                ripunzip::UnzipEngine::for_file(file).map_err(|e| format!("read failed: {e}"))?
            }
        };
        let compressed = engine.zip_length();
        if compressed > MAX_COMPRESSED_BYTES {
            return Err(format!(
                "zip too large ({} bytes; limit {})",
                compressed, MAX_COMPRESSED_BYTES
            ));
        }
        // ripunzip wants a `Box<dyn UnzipProgressReporter + Sync + 'b>`; we own
        // an Arc and need a stable address it can borrow. Move a clone of the
        // Arc into the box.
        let reporter_box: Box<dyn ripunzip::UnzipProgressReporter + Sync> =
            Box::new(ReporterHandle(reporter_for_blocking));
        let opts = ripunzip::UnzipOptions {
            output_directory: Some(extracted_for_blocking),
            password: None,
            single_threaded: false,
            filename_filter: None,
            progress_reporter: reporter_box,
        };
        engine
            .unzip(opts)
            .map_err(|e| format!("extract failed: {e}"))
    })
    .await
    .map_err(|e| format!("install task panicked: {e}"))?;

    if let Err(e) = unzip_result {
        cleanup(&work_dir);
        return Err(e);
    }

    // Phase 2: validate.
    emit_progress(
        &app,
        &InstallProgress::Validate {
            install_id: install_id.clone(),
        },
    );
    let root = match determine_root(&extracted) {
        Ok(r) => r,
        Err(e) => {
            cleanup(&work_dir);
            return Err(e);
        }
    };
    if let Err(e) = WallpaperManifest::load(root.clone()) {
        cleanup(&work_dir);
        return Err(format!("index.toml invalid: {e}"));
    }
    if !root.join("index.html").is_file() {
        cleanup(&work_dir);
        return Err("index.html missing from wallpaper root".into());
    }

    // Phase 3: move into place.
    let target_dir = match target_wallpaper_dir(&app) {
        Ok(d) => d,
        Err(e) => {
            cleanup(&work_dir);
            return Err(e);
        }
    };
    let dest = target_dir.join(&name);
    if let Err(e) = move_dir(&root, &dest) {
        cleanup(&work_dir);
        return Err(format!("move failed: {e}"));
    }
    cleanup(&work_dir);
    Ok(())
}

// Newtype so we can implement the foreign trait on an Arc-held Reporter
// without conflicting blanket impls.
struct ReporterHandle(Arc<Reporter>);

impl ripunzip::UnzipProgressReporter for ReporterHandle {
    fn total_bytes_expected(&self, expected: u64) {
        self.0.total_bytes_expected(expected);
    }
    fn bytes_extracted(&self, count: u64) {
        self.0.bytes_extracted(count);
    }
    fn extraction_starting(&self, display_name: &str) {
        self.0.extraction_starting(display_name);
    }
    fn extraction_finished(&self, display_name: &str) {
        self.0.extraction_finished(display_name);
    }
}
