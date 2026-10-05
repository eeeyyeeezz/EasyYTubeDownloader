//! Download queue: runs yt-dlp jobs with a concurrency limit and reports
//! progress to the UI through `job-update` events.

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, oneshot, Semaphore},
};

use crate::{engine::Engine, settings::SettingsStore};

pub const PRESETS: &[&str] = &["best", "1080", "720", "mp3", "m4a"];

const LOG_LINES: usize = 80;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub preset: String,
    pub dir: String,
    pub playlist: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdate {
    pub id: u64,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

impl JobUpdate {
    fn new(id: u64, status: &'static str) -> Self {
        Self {
            id,
            status,
            ..Default::default()
        }
    }
}

pub struct Downloader {
    next_id: AtomicU64,
    slots: Arc<Semaphore>,
    limit: Mutex<usize>,
    cancels: Mutex<HashMap<u64, oneshot::Sender<()>>>,
}

impl Downloader {
    pub fn new(limit: usize) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            slots: Arc::new(Semaphore::new(limit)),
            limit: Mutex::new(limit),
            cancels: Mutex::new(HashMap::new()),
        }
    }

    /// Changes the number of parallel downloads; running jobs are not affected.
    pub fn set_limit(&self, new_limit: usize) {
        let mut limit = self.limit.lock().unwrap();
        if new_limit > *limit {
            self.slots.add_permits(new_limit - *limit);
        } else if new_limit < *limit {
            let slots = self.slots.clone();
            let diff = (*limit - new_limit) as u32;
            tauri::async_runtime::spawn(async move {
                if let Ok(p) = slots.acquire_many_owned(diff).await {
                    p.forget();
                }
            });
        }
        *limit = new_limit;
    }

    pub fn start(&self, app: &AppHandle, req: DownloadRequest) -> Result<u64, String> {
        let url = url::Url::parse(req.url.trim()).map_err(|_| "invalid-url".to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("invalid-url".into());
        }
        if !PRESETS.contains(&req.preset.as_str()) {
            return Err("invalid-preset".into());
        }
        let dir = PathBuf::from(&req.dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancels.lock().unwrap().insert(id, cancel_tx);
        let _ = app.emit("job-update", JobUpdate::new(id, "queued"));

        let app = app.clone();
        let slots = self.slots.clone();
        let settings = app.state::<SettingsStore>().get();
        let cookies = settings.cookies_browser;
        let proxy = if settings.proxy.is_empty() {
            crate::system::system_proxy()
        } else {
            Some(settings.proxy)
        };
        let job = Job {
            id,
            url: url.to_string(),
            preset: req.preset,
            dir,
            playlist: req.playlist,
            cookies_browser: (!cookies.is_empty()).then_some(cookies),
            proxy,
        };
        tauri::async_runtime::spawn(async move {
            let mut cancel_rx = cancel_rx;
            let permit = tokio::select! {
                p = slots.acquire_owned() => p.ok(),
                _ = &mut cancel_rx => None,
            };
            let update = match permit {
                Some(_permit) => job.run(&app, cancel_rx).await,
                None => JobUpdate::new(id, "cancelled"),
            };
            app.state::<Downloader>().cancels.lock().unwrap().remove(&id);
            let _ = app.emit("job-update", update);
        });
        Ok(id)
    }

    pub fn cancel(&self, id: u64) {
        if let Some(tx) = self.cancels.lock().unwrap().remove(&id) {
            let _ = tx.send(());
        }
    }
}

struct Job {
    id: u64,
    url: String,
    preset: String,
    dir: PathBuf,
    playlist: bool,
    cookies_browser: Option<String>,
    proxy: Option<String>,
}

enum Line {
    Out(String),
    Err(String),
}

impl Job {
    fn args(&self, engine: &Engine) -> Vec<String> {
        let mut a: Vec<String> = vec![
            "--ignore-config",
            "--newline",
            "--progress",
            "--no-simulate",
            "--no-mtime",
            "--embed-metadata",
            "--print",
            "before_dl:EYTD_INFO %(.{id,title,thumbnail,playlist_index,n_entries})j",
            "--print",
            "after_move:EYTD_FILE %(filepath)s",
            "--progress-template",
            "download:EYTD_PROG %(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(progress.status)s",
            "--progress-template",
            "postprocess:EYTD_PP %(progress.postprocessor)s",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        if self.playlist {
            // YouTube rate-limits sessions that fetch many videos quickly
            // ("This content isn't available, try again later").
            a.extend(
                ["--yes-playlist", "--sleep-requests", "0.75", "--sleep-interval", "5", "--max-sleep-interval", "10"]
                    .map(String::from),
            );
        } else {
            a.push("--no-playlist".into());
        }
        a.extend(["-P".into(), self.dir.to_string_lossy().into_owned()]);
        a.extend([
            "-o".into(),
            "%(playlist_title&{}/|)s%(playlist_index&{} - |)s%(title).150B [%(id)s].%(ext)s".into(),
        ]);
        if let Some(loc) = engine.ffmpeg_location() {
            a.extend(["--ffmpeg-location".into(), loc.to_string_lossy().into_owned()]);
        }
        if let Some(proxy) = &self.proxy {
            a.extend(["--proxy".into(), proxy.clone()]);
        }
        if let Some(browser) = &self.cookies_browser {
            a.extend(["--cookies-from-browser".into(), browser.clone()]);
        }
        a.extend([
            "--js-runtimes".into(),
            format!("deno:{}", engine.deno().to_string_lossy()),
        ]);

        let preset: &[&str] = match self.preset.as_str() {
            "1080" => &["-f", "bv*+ba/b", "-S", "res:1080,vcodec:h264,acodec:m4a", "--merge-output-format", "mp4"],
            "720" => &["-f", "bv*+ba/b", "-S", "res:720,vcodec:h264,acodec:m4a", "--merge-output-format", "mp4"],
            "mp3" => &["-f", "ba/b", "-x", "--audio-format", "mp3", "--audio-quality", "0", "--embed-thumbnail", "--convert-thumbnails", "jpg"],
            "m4a" => &["-f", "ba[ext=m4a]/ba/b", "-x", "--audio-format", "m4a", "--embed-thumbnail", "--convert-thumbnails", "jpg"],
            _ => &["-f", "bv*+ba/b", "--merge-output-format", "mp4"],
        };
        a.extend(preset.iter().map(|s| s.to_string()));
        a.push("--".into());
        a.push(self.url.clone());
        a
    }

    async fn run(self, app: &AppHandle, mut cancel_rx: oneshot::Receiver<()>) -> JobUpdate {
        let id = self.id;
        let engine = app.state::<Engine>();
        let emit = |u: JobUpdate| {
            let _ = app.emit("job-update", u);
        };
        emit(JobUpdate::new(id, "starting"));

        let spawned = {
            let _guard = engine.lock.read().await;
            crate::proc::command(&engine.ytdlp())
                .args(self.args(&engine))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
        };
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let mut u = JobUpdate::new(id, "error");
                u.error = Some("engine".into());
                u.log = Some(e.to_string());
                return u;
            }
        };
        let pid = child.id();

        let (tx, mut rx) = mpsc::unbounded_channel();
        if let Some(out) = child.stdout.take() {
            spawn_reader(out, tx.clone(), Line::Out);
        }
        if let Some(err) = child.stderr.take() {
            spawn_reader(err, tx.clone(), Line::Err);
        }
        drop(tx);

        let mut log: VecDeque<String> = VecDeque::new();
        let mut last_error: Option<String> = None;
        let mut video_ids: Vec<String> = Vec::new();
        let mut last_file: Option<String> = None;
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut cancelled = false;

        loop {
            tokio::select! {
                line = rx.recv() => {
                    let Some(line) = line else { break };
                    match line {
                        Line::Out(l) => {
                            if let Some(json) = l.strip_prefix("EYTD_INFO ") {
                                let mut u = JobUpdate::new(id, "downloading");
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                                    if let Some(vid) = v["id"].as_str() {
                                        video_ids.push(vid.to_string());
                                    }
                                    u.title = v["title"].as_str().map(String::from);
                                    u.thumbnail = v["thumbnail"].as_str().map(String::from);
                                    u.item = v["playlist_index"].as_u64().map(|n| n as u32);
                                    u.items = v["n_entries"].as_u64().map(|n| n as u32);
                                }
                                u.percent = Some(0.0);
                                emit(u);
                            } else if let Some(p) = l.strip_prefix("EYTD_PROG ") {
                                let finished = p.ends_with("|finished");
                                if finished || last_emit.elapsed() > Duration::from_millis(250) {
                                    emit(parse_progress(id, p));
                                    last_emit = Instant::now();
                                }
                            } else if l.starts_with("EYTD_PP ") {
                                emit(JobUpdate::new(id, "processing"));
                            } else if let Some(f) = l.strip_prefix("EYTD_FILE ") {
                                last_file = Some(f.trim().to_string());
                            } else {
                                push_log(&mut log, l);
                            }
                        }
                        Line::Err(l) => {
                            if l.starts_with("ERROR:") {
                                last_error = Some(l.clone());
                            }
                            push_log(&mut log, l);
                        }
                    }
                }
                _ = &mut cancel_rx, if !cancelled => {
                    cancelled = true;
                    if let Some(pid) = pid {
                        crate::proc::kill_tree(pid).await;
                    }
                    let _ = child.start_kill();
                }
            }
        }

        let status = child.wait().await;
        if cancelled {
            remove_partials(&self.dir, &video_ids);
            return JobUpdate::new(id, "cancelled");
        }
        match status {
            Ok(s) if s.success() => {
                let mut u = JobUpdate::new(id, "done");
                u.percent = Some(100.0);
                u.file = last_file.or_else(|| Some(self.dir.to_string_lossy().into_owned()));
                u
            }
            _ => {
                let mut u = JobUpdate::new(id, "error");
                u.error = Some(classify(last_error.as_deref().unwrap_or("")).into());
                u.log = Some(log.into_iter().collect::<Vec<_>>().join("\n"));
                u
            }
        }
    }
}

fn spawn_reader<R>(reader: R, tx: mpsc::UnboundedSender<Line>, wrap: fn(String) -> Line)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).split(b'\n');
        while let Ok(Some(bytes)) = lines.next_segment().await {
            let line = String::from_utf8_lossy(&bytes).trim_end_matches('\r').to_string();
            if tx.send(wrap(line)).is_err() {
                break;
            }
        }
    });
}

fn push_log(log: &mut VecDeque<String>, line: String) {
    if log.len() >= LOG_LINES {
        log.pop_front();
    }
    log.push_back(line);
}

fn num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

fn parse_progress(id: u64, p: &str) -> JobUpdate {
    let f: Vec<&str> = p.split('|').collect();
    let get = |i: usize| f.get(i).copied().and_then(num);
    let downloaded = get(0);
    let total = get(1).or(get(2));
    let mut u = JobUpdate::new(id, "downloading");
    u.percent = match (downloaded, total) {
        (Some(d), Some(t)) if t > 0.0 => Some((d / t * 100.0).min(100.0)),
        _ => None,
    };
    if f.get(5) == Some(&"finished") {
        u.percent = Some(100.0);
    }
    u.speed = get(3);
    u.eta = get(4);
    u
}

/// Maps a yt-dlp error message to a code the UI can translate.
fn classify(err: &str) -> &'static str {
    let e = err.to_lowercase();
    // Problems reading browser cookies come first: they explain any later failure.
    if e.contains("could not copy") && e.contains("cookie database") {
        "cookies-locked"
    } else if e.contains("failed to decrypt with dpapi") || e.contains("app-bound encryption") {
        "cookies-decrypt"
    } else if e.contains("operation not permitted") && e.contains("cookies") {
        "cookies-permission"
    } else if e.contains("could not find") && e.contains("cookies database") {
        "cookies-missing"
    } else if e.contains("confirm your age") || e.contains("age-restricted") || e.contains("age restricted") {
        "age"
    } else if e.contains("not a bot") {
        "bot"
    } else if e.contains("private video") {
        "private"
    } else if e.contains("members-only") || e.contains("join this channel") {
        "members"
    } else if e.contains("unsupported url") {
        "unsupported"
    } else if e.contains("video unavailable") || e.contains("is not available") {
        "unavailable"
    } else if e.contains("no space left") {
        "disk"
    } else if e.contains("getaddrinfo")
        || e.contains("timed out")
        || e.contains("unable to download webpage")
        || e.contains("connection")
        || e.contains("network")
    {
        "network"
    } else {
        "unknown"
    }
}

/// Deletes leftovers (`.part`, `.ytdl`, unmerged `.fNNN` streams) of a cancelled job.
fn remove_partials(dir: &Path, ids: &[String]) {
    fn walk(dir: &Path, depth: u8, ids: &[String]) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth > 0 {
                    walk(&path, depth - 1, ids);
                }
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let leftover = ids.iter().any(|id| {
                let tag = format!("[{id}]");
                let Some(pos) = name.find(&tag) else { return false };
                let rest = &name[pos + tag.len()..];
                rest.contains(".part")
                    || rest.ends_with(".ytdl")
                    || rest.contains(".temp")
                    || rest
                        .strip_prefix(".f")
                        .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
            });
            if leftover {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    walk(dir, 1, ids);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_parsing() {
        let u = parse_progress(1, "50|200|NA|1024.5|7|downloading");
        assert_eq!(u.percent, Some(25.0));
        assert_eq!(u.speed, Some(1024.5));
        assert_eq!(u.eta, Some(7.0));
        let u = parse_progress(1, "50|NA|100|NA|NA|downloading");
        assert_eq!(u.percent, Some(50.0));
        let u = parse_progress(1, "NA|NA|NA|NA|NA|finished");
        assert_eq!(u.percent, Some(100.0));
    }

    #[test]
    fn error_classification() {
        assert_eq!(classify("ERROR: [youtube] x: Private video. Sign in"), "private");
        assert_eq!(classify("ERROR: [youtube] x: Sign in to confirm you’re not a bot"), "bot");
        assert_eq!(classify("ERROR: [youtube] x: Video unavailable"), "unavailable");
        assert_eq!(classify("ERROR: something odd"), "unknown");
        assert_eq!(
            classify("ERROR: Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271"),
            "cookies-locked"
        );
        assert_eq!(classify("ERROR: Failed to decrypt with DPAPI. See https://x"), "cookies-decrypt");
        assert_eq!(
            classify("ERROR: [Errno 1] Operation not permitted: '/Users/a/Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies'"),
            "cookies-permission"
        );
        assert_eq!(classify("ERROR: could not find firefox cookies database in /x"), "cookies-missing");
    }

    #[test]
    fn partials_cleanup() {
        let dir = std::env::temp_dir().join(format!("eytd-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let keep = ["Song [abc].mp4", "Other [zzz].mp4.part"];
        let drop = ["Song [abc].mp4.part", "Song [abc].f137.mp4", "Song [abc].f140.m4a.part-Frag3", "Song [abc].mp4.ytdl"];
        for n in keep.iter().chain(drop.iter()) {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        remove_partials(&dir, &["abc".to_string()]);
        for n in keep {
            assert!(dir.join(n).exists(), "{n} should stay");
        }
        for n in drop {
            assert!(!dir.join(n).exists(), "{n} should be removed");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
