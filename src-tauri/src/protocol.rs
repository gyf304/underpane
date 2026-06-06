use crate::config::{expand_tilde, Scalar, CONFIG};
use crate::desktop_windows::realm_origin;
use crate::wallpapers::{WallpaperConfigSchema, WallpaperManifest};
use percent_encoding::percent_decode_str;
use std::path::PathBuf;

const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// Higher full-read cap for user-chosen external `file` inputs (images/video),
/// which are commonly larger than bundled assets. Range requests still stream
/// in `MAX_RESPONSE_BYTES`-sized chunks regardless of this cap.
const MAX_EXTERNAL_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
const CSP: &str = "default-src 'self' 'unsafe-inline' ipc: http://ipc.localhost";

/// A parsed `underpane://` host of the form `monitor-{i1}.{id}.{realm}` (with an
/// optional leading `underpane` scheme segment on Windows). Anchoring on the
/// numeric `monitor-{n}` segment makes the dot-free `id`/`realm` segments and the
/// Windows prefix parse unambiguously.
struct Target {
    realm: String,
    index: usize,
    id: String,
}

fn parse_host(host: &str) -> Option<Target> {
    let parts: Vec<&str> = host.split('.').collect();
    // Locate the `monitor-{n}` segment (1-based, n >= 1).
    let p = parts.iter().position(|s| {
        s.strip_prefix("monitor-")
            .and_then(|n| n.parse::<usize>().ok())
            .is_some_and(|n| n > 0)
    })?;
    let index = parts[p]
        .strip_prefix("monitor-")
        .and_then(|n| n.parse::<usize>().ok())?
        - 1;
    let id = parts.get(p + 1).copied().filter(|s| !s.is_empty())?;
    let realm = parts.get(p + 2).copied().filter(|s| !s.is_empty())?;
    Some(Target {
        realm: realm.to_string(),
        index,
        id: id.to_string(),
    })
}

/// Adds permissive CORS headers so the wallpaper document can load `asset`-realm
/// files from its sibling origin, including ranged `<video>`/`fetch` requests.
fn add_cors(response: &mut tauri::http::Response<Vec<u8>>) {
    let h = response.headers_mut();
    h.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    h.insert(
        "Access-Control-Allow-Methods",
        "GET, HEAD, OPTIONS".parse().unwrap(),
    );
    h.insert("Access-Control-Allow-Headers", "Range".parse().unwrap());
    h.insert(
        "Access-Control-Expose-Headers",
        "Content-Length, Content-Range, Accept-Ranges, Content-Type"
            .parse()
            .unwrap(),
    );
}

pub async fn handle(request: tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    let target = request.uri().host().and_then(parse_host);

    // Answer CORS preflights (cross-origin ranged `fetch` from the wallpaper page).
    if *request.method() == tauri::http::Method::OPTIONS {
        let mut resp = tauri::http::Response::builder()
            .status(204)
            .body(vec![])
            .unwrap();
        add_cors(&mut resp);
        return resp;
    }

    let mut response = handle_inner(&request, target.as_ref()).await;

    match target.as_ref().map(|t| t.realm.as_str()) {
        // The wallpaper document loads its `file` inputs from the sibling `asset`
        // origin, so its CSP must allow that origin in addition to `'self'`.
        Some("wallpaper") => {
            let t = target.as_ref().unwrap();
            let asset = realm_origin("asset", t.index, &t.id);
            let csp = format!("{CSP} {asset}");
            response
                .headers_mut()
                .insert("Content-Security-Policy", csp.parse().unwrap());
        }
        Some("asset") => add_cors(&mut response),
        _ => {}
    }
    response
}

fn not_found() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .header("Content-Type", "text/plain")
        .body(b"Not Found".to_vec())
        .unwrap()
}

async fn handle_inner(
    request: &tauri::http::Request<Vec<u8>>,
    target: Option<&Target>,
) -> tauri::http::Response<Vec<u8>> {
    let Some(target) = target else {
        return not_found();
    };

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

    let mut path = request.uri().path().trim_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    let (resolved, max_bytes) = match target.realm.as_str() {
        "wallpaper" => (
            resolve_wallpaper_file(&target.id, &path).await,
            MAX_RESPONSE_BYTES,
        ),
        // Bare `{input-id}/{filename}` path within the wallpaper's `asset` realm.
        "asset" => (
            resolve_external_file(target.index, &target.id, &path).await,
            MAX_EXTERNAL_RESPONSE_BYTES,
        ),
        _ => return not_found(),
    };
    let Some((full_path, metadata)) = resolved else {
        return not_found();
    };
    serve_file(request, &full_path, &metadata, max_bytes).await
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

/// Resolves an `asset`-realm `{input-id}/{filename}` path to the on-disk file
/// selected for that `file` input on the given monitor. Only inputs declared as
/// `file` in the wallpaper manifest are served, and the disk path comes from
/// config (never from the URL), so the URL can't traverse the filesystem.
async fn resolve_external_file(
    index: usize,
    wallpaper: &str,
    rest: &str,
) -> Option<(PathBuf, std::fs::Metadata)> {
    let raw = rest.split('/').next().filter(|s| !s.is_empty())?;
    // The input id is percent-encoded in the URL; decode before matching config keys.
    let input_id = percent_decode_str(raw).decode_utf8().ok()?;
    let input_id = input_id.as_ref();

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
