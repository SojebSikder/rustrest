//! Host-side management of outbound HTTP requests a plugin starts via the
//! `ExternalProcess` capability's `http_request` host call.

use rustrest_plugin_api::HttpResponseData;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_RESPONSE_BYTES: u64 = 20 * 1024 * 1024; // 20 MB
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024; // 200 MB
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// a plugin download in flight, for the host UI to show progress
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub plugin_id: String,
    pub file_name: String,
    pub downloaded: u64,
    /// from Content-Length, when the server sends one
    pub total: Option<u64>,
}

/// every download running right now, across all plugins and threads
static ACTIVE_DOWNLOADS: Mutex<Vec<(u64, DownloadProgress)>> = Mutex::new(Vec::new());
static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(0);

/// snapshot of the downloads in flight, oldest first
pub fn active_downloads() -> Vec<DownloadProgress> {
    ACTIVE_DOWNLOADS
        .lock()
        .map(|list| list.iter().map(|(_, p)| p.clone()).collect())
        .unwrap_or_default()
}

/// keeps a download listed in `ACTIVE_DOWNLOADS` until dropped, so it's removed however `fetch_to_file` returns
struct DownloadEntry(u64);

impl DownloadEntry {
    fn start(progress: DownloadProgress) -> Self {
        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut list) = ACTIVE_DOWNLOADS.lock() {
            list.push((id, progress));
        }
        Self(id)
    }

    fn set_downloaded(&self, downloaded: u64) {
        if let Ok(mut list) = ACTIVE_DOWNLOADS.lock()
            && let Some((_, progress)) = list.iter_mut().find(|(id, _)| *id == self.0)
        {
            progress.downloaded = downloaded;
        }
    }
}

impl Drop for DownloadEntry {
    fn drop(&mut self) {
        if let Ok(mut list) = ACTIVE_DOWNLOADS.lock() {
            list.retain(|(id, _)| *id != self.0);
        }
    }
}

pub enum NetworkEvent {
    /// one line of the response body, delivered as soon as it's read off
    /// the socket (before the response is complete) - lets a plugin render
    /// a streamed reply (e.g. an SSE/NDJSON chat completion) incrementally
    /// instead of waiting for the whole body.
    Chunk(u32, Vec<u8>),
    Response(u32, Result<HttpResponseData, String>),
    /// an archive started via `download_archive` finished downloading and
    /// extracting, holds every extracted file's absolute path.
    Download(u32, Result<Vec<String>, String>),
}

#[derive(Default)]
pub struct NetworkTable {
    next_handle: u32,
    events: Vec<NetworkEvent>,
}

impl NetworkTable {
    /// starts `method url` with `headers`/`body` on a background thread,
    /// returning a handle immediately. `https://` only.
    pub fn spawn_request(
        shared: &Arc<Mutex<NetworkTable>>,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<u32, String> {
        if !url.starts_with("https://") {
            return Err("only https:// urls are allowed".to_string());
        }

        let handle = {
            let mut table = shared.lock().expect("network table poisoned");
            table.next_handle += 1;
            table.next_handle
        };

        let shared = shared.clone();
        std::thread::spawn(move || {
            let result = run_request(handle, &shared, &method, &url, &headers, body.as_deref());
            let mut table = shared.lock().expect("network table poisoned");
            table.events.push(NetworkEvent::Response(handle, result));
        });

        Ok(handle)
    }

    /// downloads the archive at `url` on a background thread, verifies it
    /// against `checksum_url` (if given), and extracts it into `storage_dir/dest_dir`,
    /// replacing that directory. Returns a handle immediately, and completion arrives as `NetworkEvent::Download`.
    pub fn spawn_download(
        shared: &Arc<Mutex<NetworkTable>>,
        plugin_id: String,
        storage_dir: PathBuf,
        url: String,
        checksum_url: Option<String>,
        dest_dir: String,
    ) -> Result<u32, String> {
        if !url.starts_with("https://")
            || checksum_url
                .as_ref()
                .is_some_and(|u| !u.starts_with("https://"))
        {
            return Err("only https:// urls are allowed".to_string());
        }
        let dest_name = Path::new(&dest_dir)
            .file_name()
            .filter(|n| Path::new(n) == Path::new(&dest_dir))
            .ok_or_else(|| "dest_dir must be a single directory name".to_string())?
            .to_owned();

        let handle = {
            let mut table = shared.lock().expect("network table poisoned");
            table.next_handle += 1;
            table.next_handle
        };

        let shared = shared.clone();
        std::thread::spawn(move || {
            let result = download_archive(
                &plugin_id,
                &storage_dir,
                &url,
                checksum_url.as_deref(),
                &storage_dir.join(dest_name),
            );
            let mut table = shared.lock().expect("network table poisoned");
            table.events.push(NetworkEvent::Download(handle, result));
        });

        Ok(handle)
    }

    /// drains buffered response events; called by the pump on a timer.
    pub fn drain_events(shared: &Arc<Mutex<NetworkTable>>) -> Vec<NetworkEvent> {
        let mut table = shared.lock().expect("network table poisoned");
        std::mem::take(&mut table.events)
    }
}

fn run_request(
    handle: u32,
    shared: &Arc<Mutex<NetworkTable>>,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<HttpResponseData, String> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    let mut request = client.request(method, url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some(bytes) = body {
        request = request.body(bytes.to_vec());
    }

    let response = request.send().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let response_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();

    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES)
    {
        return Err("response exceeds maximum allowed size".to_string());
    }

    // read line-by-line rather than all at once - a streamed reply (SSE or
    // NDJSON, as chat-completion APIs use) flushes one line per event, so
    // this lets the caller push a `Chunk` per line as it arrives instead of
    // blocking until the whole body has been received. Harmless for a
    // non-streamed body: it just reads out as one final "line".
    let mut reader = BufReader::new(response);
    let mut full_body = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        full_body.extend_from_slice(&line);
        if full_body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("response exceeds maximum allowed size".to_string());
        }
        let mut table = shared.lock().expect("network table poisoned");
        table.events.push(NetworkEvent::Chunk(handle, line.clone()));
    }

    Ok(HttpResponseData {
        status,
        headers: response_headers,
        body: full_body,
    })
}

/// streams `url` into `dest`, listed in `active_downloads` under `plugin_id` while it runs
pub fn fetch_to_file(url: &str, dest: &Path, plugin_id: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https:// urls are allowed".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(url).send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "download failed: HTTP {} ({url})",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_DOWNLOAD_BYTES)
    {
        return Err("download exceeds maximum allowed size".to_string());
    }

    let entry = DownloadEntry::start(DownloadProgress {
        plugin_id: plugin_id.to_string(),
        file_name: dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        downloaded: 0,
        total: response.content_length(),
    });

    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut written: u64 = 0;
    let mut reported: u64 = 0;
    let mut buf = [0u8; 8192];
    loop {
        let n = response.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        written += n as u64;
        // the UI polls a few times a second, no need to take the lock per chunk
        if written - reported >= 256 * 1024 {
            entry.set_downloaded(written);
            reported = written;
        }
        if written > MAX_DOWNLOAD_BYTES {
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err("download exceeds maximum allowed size".to_string());
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn download_archive(
    plugin_id: &str,
    storage_dir: &Path,
    url: &str,
    checksum_url: Option<&str>,
    dest: &Path,
) -> Result<Vec<String>, String> {
    // keep the archive's own file name so extraction can detect its format
    let archive_name = url
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "url has no file name".to_string())?;
    let tmp_dir = storage_dir.join(format!(".download-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    let result = (|| {
        let archive_path = tmp_dir.join(archive_name);
        fetch_to_file(url, &archive_path, plugin_id)?;

        if let Some(checksum_url) = checksum_url {
            let checksum_path = tmp_dir.join("checksum");
            fetch_to_file(checksum_url, &checksum_path, plugin_id)?;
            verify_sha256(&archive_path, &checksum_path)?;
        }

        if dest.exists() {
            std::fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        self_update::Extract::from_source(&archive_path)
            .extract_into(dest)
            .map_err(|e| format!("failed to extract {archive_name}: {e}"))?;

        let mut files = Vec::new();
        collect_files(dest, &mut files);
        Ok(files)
    })();

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

/// `checksum_path` holds the expected hex digest as its first word
/// (`sha256sum`-style `<hex>  <file>` lines are fine).
fn verify_sha256(archive_path: &Path, checksum_path: &Path) -> Result<(), String> {
    let expected = std::fs::read_to_string(checksum_path).map_err(|e| e.to_string())?;
    let expected = expected
        .split_whitespace()
        .next()
        .ok_or_else(|| "empty checksum file".to_string())?;

    let mut file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();

    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;

    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch: expected {expected}, got {actual}"
        ))
    }
}

fn collect_files(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path.to_string_lossy().to_string());
        }
    }
}
