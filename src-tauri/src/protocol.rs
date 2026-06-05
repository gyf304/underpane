use crate::config::{expand_tilde, Scalar, CONFIG};
use crate::wallpapers::{WallpaperConfigSchema, WallpaperManifest};
use std::path::PathBuf;

const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// Higher full-read cap for user-chosen external `file` inputs (images/video),
/// which are commonly larger than bundled assets. Range requests still stream
/// in `MAX_RESPONSE_BYTES`-sized chunks regardless of this cap.
const MAX_EXTERNAL_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
const EXTERNAL_FILE_PREFIX: &str = ".underpane/external-file/";
const CSP: &str = "default-src 'self' 'unsafe-inline' ipc: http://ipc.localhost";

pub async fn handle(request: tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    let mut response = handle_inner(request).await;
    response
        .headers_mut()
        .insert("Content-Security-Policy", CSP.parse().unwrap());
    response
}

fn not_found() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .header("Content-Type", "text/plain")
        .body(b"Not Found".to_vec())
        .unwrap()
}

async fn handle_inner(request: tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    let uri = request.uri();
    // The host is keyed by both monitor and wallpaper as `monitor-{n}.{wallpaper}`.
    // Strip the `monitor-{n}.` prefix; the remainder is the wallpaper id.
    let host = uri.host().unwrap();
    let Some((monitor_part, wallpaper)) = host.split_once('.') else {
        return not_found();
    };
    // `monitor-{n}` is 1-based; convert to a 0-based monitor index.
    let Some(index) = monitor_part
        .strip_prefix("monitor-")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .map(|n| n - 1)
    else {
        return not_found();
    };
    if wallpaper.is_empty() {
        return not_found();
    }
    let mut path = uri.path().trim_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    match *request.method() {
        tauri::http::Method::GET | tauri::http::Method::HEAD => {}
        _ => {
            return tauri::http::Response::builder()
                .status(405)
                .header("Allow", "GET, HEAD")
                .body(vec![])
                .unwrap();
        }
    }

    let (resolved, max_bytes) = if let Some(rest) = path.strip_prefix(EXTERNAL_FILE_PREFIX) {
        (
            resolve_external_file(index, wallpaper, rest).await,
            MAX_EXTERNAL_RESPONSE_BYTES,
        )
    } else {
        (
            resolve_wallpaper_file(wallpaper, &path).await,
            MAX_RESPONSE_BYTES,
        )
    };
    let Some((full_path, metadata)) = resolved else {
        return not_found();
    };
    serve_file(&request, &full_path, &metadata, max_bytes).await
}

/// Resolves a normal asset path within the wallpaper's directory, searching the
/// configured wallpaper directories in order.
async fn resolve_wallpaper_file(
    wallpaper: &str,
    path: &str,
) -> Option<(PathBuf, std::fs::Metadata)> {
    let dirs = CONFIG.borrow().get_wallpaper_dirs();
    for base in &dirs {
        let candidate = base.join(wallpaper).join(path);
        if let Ok(m) = tokio::fs::metadata(&candidate).await {
            return Some((candidate, m));
        }
    }
    None
}

/// Resolves a virtual `.underpane/external-file/{input-id}/{filename}` path to
/// the on-disk file selected for that `file` input on the given monitor. Only
/// inputs declared as `file` in the wallpaper manifest are served, and the disk
/// path comes from config (never from the URL), so the URL can't traverse the
/// filesystem.
async fn resolve_external_file(
    index: usize,
    wallpaper: &str,
    rest: &str,
) -> Option<(PathBuf, std::fs::Metadata)> {
    let input_id = rest.split('/').next().filter(|s| !s.is_empty())?;

    let manifest = WallpaperManifest::get(wallpaper).ok()?;
    if !matches!(
        manifest.config.get(input_id),
        Some(WallpaperConfigSchema::File { .. })
    ) {
        return None;
    }

    // Clone the stored path out before any `.await` so the CONFIG borrow guard
    // isn't held across an await point.
    let disk_path = {
        let cfg = CONFIG.borrow();
        let monitor_config = cfg.get_monitor_config(index)?;
        match monitor_config.config.get(input_id) {
            Some(Scalar::String(s)) if !s.is_empty() => expand_tilde(s),
            _ => return None,
        }
    };

    let metadata = tokio::fs::metadata(&disk_path).await.ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some((disk_path, metadata))
}

async fn serve_file(
    request: &tauri::http::Request<Vec<u8>>,
    full_path: &PathBuf,
    metadata: &std::fs::Metadata,
    max_bytes: u64,
) -> tauri::http::Response<Vec<u8>> {
    let file_size = metadata.len();
    let mime = mime_guess::from_path(&full_path).first_or_octet_stream();

    if file_size > max_bytes {
        return tauri::http::Response::builder()
            .status(413)
            .header("Accept-Ranges", "bytes")
            .body(vec![])
            .unwrap();
    }

    if request.method() == tauri::http::Method::HEAD {
        return tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", mime.as_ref())
            .header("Content-Length", file_size.to_string())
            .header("Accept-Ranges", "bytes")
            .body(vec![])
            .unwrap();
    }

    if let Some(range) = request.headers().get("range").and_then(|v| v.to_str().ok()) {
        if let Some(spec) = range.strip_prefix("bytes=") {
            let mut parts = spec.splitn(2, '-');
            let a = parts.next().unwrap_or("");
            let b = parts.next().unwrap_or("");
            let (start, end) = if a.is_empty() {
                let s: u64 = b.parse().unwrap_or(0);
                (file_size.saturating_sub(s), file_size - 1)
            } else {
                let s: u64 = a.parse().unwrap_or(0);
                let e: u64 = if b.is_empty() {
                    file_size - 1
                } else {
                    b.parse().unwrap_or(file_size - 1).min(file_size - 1)
                };
                (s, e)
            };
            let end = end.min(start.saturating_add(MAX_RESPONSE_BYTES - 1));
            if start <= end && start < file_size {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                if let Ok(mut f) = tokio::fs::File::open(&full_path).await {
                    f.seek(std::io::SeekFrom::Start(start)).await.ok();
                    let len = (end - start + 1) as usize;
                    let mut buf = vec![0u8; len];
                    f.read_exact(&mut buf).await.ok();
                    return tauri::http::Response::builder()
                        .status(206)
                        .header("Content-Type", mime.as_ref())
                        .header("Content-Length", len.to_string())
                        .header("Content-Range", format!("bytes {start}-{end}/{file_size}"))
                        .header("Accept-Ranges", "bytes")
                        .body(buf)
                        .unwrap();
                }
            }
        }
        return tauri::http::Response::builder()
            .status(416)
            .header("Content-Range", format!("bytes */{file_size}"))
            .body(vec![])
            .unwrap();
    }

    match tokio::fs::read(&full_path).await {
        Ok(b) => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", mime.as_ref())
            .header("Content-Length", b.len().to_string())
            .header("Accept-Ranges", "bytes")
            .body(b)
            .unwrap(),
        Err(_) => not_found(),
    }
}
