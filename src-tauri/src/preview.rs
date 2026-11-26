//! Opaque audio preview protocol.
//!
//! The webview receives a URL with an opaque file id. It never receives bytes,
//! storage paths, or a Discord CDN URL. Module files render to WAV through
//! openmpt123 and the result is cached by content hash.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime};

use swap_db::{Db, FileRepo, MetaRepo};
use swap_files::{BlobStore, OpenMptRenderer, render_preview};

const PROTOCOL: &str = "swap-preview";
const PREVIEW_SECONDS: u32 = 120;

#[derive(Default)]
struct PreviewRegistry {
    by_token: HashMap<String, String>,
    by_file: HashMap<String, String>,
}

fn registry() -> &'static Mutex<PreviewRegistry> {
    static REGISTRY: OnceLock<Mutex<PreviewRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(PreviewRegistry::default()))
}

/// Register a file for the current app process and return an opaque token.
/// The token is stable for one file in one process. It never contains the
/// event, track, or storage path.
pub fn register(file_id: &str) -> String {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(token) = registry.by_file.get(file_id) {
        return token.clone();
    }
    let token = token_for(file_id);
    registry.by_token.insert(token.clone(), file_id.to_string());
    registry.by_file.insert(file_id.to_string(), token.clone());
    token
}

/// Drop every token. Tests use this to isolate the registry.
#[cfg(test)]
fn reset() {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.by_token.clear();
    registry.by_file.clear();
}

fn resolve(token: &str) -> Option<String> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .by_token
        .get(token)
        .cloned()
}

fn token_for(file_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret());
    hasher.update(file_id.as_bytes());
    let digest = hasher.finalize();
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn secret() -> &'static [u8; 32] {
    static SECRET: OnceLock<[u8; 32]> = OnceLock::new();
    SECRET.get_or_init(|| {
        let mut hasher = Sha256::new();
        hasher.update(std::process::id().to_le_bytes());
        hasher.update(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        hasher.update((std::ptr::addr_of!(SECRET) as usize).to_le_bytes());
        hasher.finalize().into()
    })
}

pub fn register_protocol<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(PROTOCOL, |context, request, responder| {
        let app = context.app_handle().clone();
        std::thread::spawn(move || {
            responder.respond(respond(&app, &request));
        });
    })
}

fn respond<R: Runtime>(app: &AppHandle<R>, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() != "GET" && request.method() != "HEAD" {
        return error(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
    }
    let Some(token) = decode_segment(request.uri().path().trim_start_matches('/')) else {
        return error(StatusCode::BAD_REQUEST, "invalid preview path");
    };
    if token.len() != 32 || !token.chars().all(|character| character.is_ascii_hexdigit()) {
        return error(StatusCode::BAD_REQUEST, "invalid preview path");
    }
    let file = match preview_file(app, &token) {
        Ok(file) => file,
        Err((status, message)) => return error(status, message),
    };
    let bytes = match preview_bytes(app, &file) {
        Ok(bytes) => bytes,
        Err((status, message)) => return error(status, message),
    };
    media_response(request, &file.extension, bytes)
}

struct PreviewFile {
    sha256: String,
    extension: String,
}

fn preview_file<R: Runtime>(
    app: &AppHandle<R>,
    token: &str,
) -> Result<PreviewFile, (StatusCode, &'static str)> {
    let file_id = resolve(token).ok_or((StatusCode::NOT_FOUND, "file not found"))?;
    let directory =
        data_dir(app).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "no data dir"))?;
    let db = Db::open(directory.join("monsquaz.db"))
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"))?;
    let file = FileRepo::get(db.conn(), &file_id)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"))?
        .ok_or((StatusCode::NOT_FOUND, "file not found"))?;
    let current = MetaRepo::get(db.conn(), "host.current_event")
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "database unavailable"))?;
    if current.as_deref() != Some(file.event_id.as_str()) {
        return Err((StatusCode::NOT_FOUND, "file not found"));
    }
    Ok(PreviewFile {
        sha256: file.sha256,
        extension: file.extension,
    })
}

fn preview_bytes<R: Runtime>(
    app: &AppHandle<R>,
    file: &PreviewFile,
) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    let directory =
        data_dir(app).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "no data dir"))?;
    let store = BlobStore::new(directory.join("files"));
    if is_audio(&file.extension) {
        return store
            .read(&file.sha256)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "file unavailable"));
    }
    if !is_module(&file.extension) {
        return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "no audio preview"));
    }
    let cache = directory
        .join("previews")
        .join(format!("{}.wav", file.sha256));
    if let Ok(bytes) = fs::read(&cache) {
        return Ok(bytes);
    }
    let bytes = store
        .read(&file.sha256)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "file unavailable"))?;
    let renderer = OpenMptRenderer::discover()
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "renderer unavailable"))?;
    let wav = render_preview(&renderer, &bytes, &file.extension, PREVIEW_SECONDS)
        .map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "render failed"))?;
    if let Some(parent) = cache.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&cache, &wav);
    Ok(wav)
}

fn media_response(
    request: &Request<Vec<u8>>,
    extension: &str,
    bytes: Vec<u8>,
) -> Response<Vec<u8>> {
    let total = bytes.len();
    let mime = mime_for(extension);
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, total));
    let head = request.method() == "HEAD";
    match range {
        Some((start, end)) => {
            let body = if head {
                Vec::new()
            } else {
                bytes[start..=end].to_vec()
            };
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total}"),
                )
                .header(header::CONTENT_LENGTH, (end - start + 1).to_string())
                .header(header::CACHE_CONTROL, "private, max-age=3600")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(body)
                .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "response failed"))
        }
        None => {
            let body = if head { Vec::new() } else { bytes };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, total.to_string())
                .header(header::CACHE_CONTROL, "private, max-age=3600")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(body)
                .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "response failed"))
        }
    }
}

fn decode_segment(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = (bytes[index + 1] as char).to_digit(16)?;
            let low = (bytes[index + 2] as char).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn parse_range(value: &str, total: usize) -> Option<(usize, usize)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    let start = start.parse::<usize>().ok()?;
    if start >= total {
        return None;
    }
    let end = end
        .parse::<usize>()
        .ok()
        .unwrap_or(total - 1)
        .min(total - 1);
    if end < start {
        return None;
    }
    Some((start, end))
}

fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|error| error.to_string())
}

fn is_module(extension: &str) -> bool {
    matches!(
        extension,
        "it" | "mptm" | "xm" | "mod" | "s3m" | "mtm" | "669"
    )
}

fn is_audio(extension: &str) -> bool {
    matches!(extension, "wav" | "flac" | "mp3" | "ogg")
}

fn mime_for(extension: &str) -> &'static str {
    match extension {
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => "audio/wav",
    }
}

fn error(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(message.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_tokens_are_opaque_and_stable() {
        let first = register("evt-1:SAAAA:1");
        let second = register("evt-2:SBBBB:1");
        assert_eq!(first, register("evt-1:SAAAA:1"));
        assert_ne!(first, second);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(first, "evt-1:SAAAA:1");
        assert_eq!(resolve(&first).as_deref(), Some("evt-1:SAAAA:1"));
        assert_eq!(resolve("not-a-token"), None);
        reset();
        assert_eq!(resolve(&first), None);
        let next = register("evt-1:SAAAA:1");
        assert_eq!(resolve(&next).as_deref(), Some("evt-1:SAAAA:1"));
    }

    #[test]
    fn preview_ids_are_decoded_and_bounded() {
        assert_eq!(
            decode_segment("evt%3AS8G4H%3A2").as_deref(),
            Some("evt:S8G4H:2")
        );
        assert_eq!(decode_segment("plain-id").as_deref(), Some("plain-id"));
        assert_eq!(decode_segment("bad%2"), None);
        assert_eq!(decode_segment("bad%zz"), None);
    }

    #[test]
    fn ranges_are_bounded() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=90-", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=90-200", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }

    #[test]
    fn only_audio_extensions_are_playable() {
        assert!(is_module("it"));
        assert!(is_audio("wav"));
        assert!(!is_module("zip"));
        assert!(!is_audio("zip"));
    }
}
