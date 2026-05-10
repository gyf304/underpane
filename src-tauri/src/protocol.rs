use crate::config::CONFIG;

const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

fn not_found() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .header("Content-Type", "text/plain")
        .body(b"Not Found".to_vec())
        .unwrap()
}

pub async fn handle(request: tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    let uri = request.uri();
    let wallpaper = uri.host().unwrap();
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

    let dirs = CONFIG.borrow().get_wallpaper_dirs();
    let mut resolved: Option<(std::path::PathBuf, std::fs::Metadata)> = None;
    for base in &dirs {
        let candidate = base.join(wallpaper).join(&path);
        if let Ok(m) = tokio::fs::metadata(&candidate).await {
            resolved = Some((candidate, m));
            break;
        }
    }
    let Some((full_path, metadata)) = resolved else {
        return not_found();
    };
    let file_size = metadata.len();
    let mime = mime_guess::from_path(&full_path).first_or_octet_stream();

    if file_size > MAX_RESPONSE_BYTES {
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
