//! Downloads, verifies and updates the external tools the app relies on:
//! yt-dlp (the downloader), ffmpeg/ffprobe (merging and audio conversion)
//! and deno (JavaScript runtime yt-dlp needs for YouTube).
//!
//! The tools live in the app data directory rather than inside the bundle so
//! that yt-dlp can update itself whenever YouTube changes something.

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::{io::AsyncWriteExt, sync::RwLock};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

#[derive(Clone, Copy)]
enum Unpack {
    /// The download is the binary itself.
    Raw,
    /// A single gzip-compressed binary.
    Gz,
    Zip,
    TarXz,
}

enum Checksum {
    /// URL of a checksum file listing `sums_name`.
    File(String),
    /// Known SHA-256 of a pinned download.
    Pinned(&'static str),
}

struct Asset {
    component: &'static str,
    url: String,
    checksum: Checksum,
    /// File name to look up in the checksum file.
    sums_name: String,
    unpack: Unpack,
    /// Binaries to place into the engine directory (without `.exe`).
    binaries: &'static [&'static str],
}

fn assets(include_ffmpeg: bool) -> Vec<Asset> {
    let ytdlp_base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
    let deno_base = "https://github.com/denoland/deno/releases/latest/download";
    let ffmpeg_base = "https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download";
    let arm = cfg!(target_arch = "aarch64");

    let ytdlp_file = if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else if cfg!(windows) {
        "yt-dlp.exe"
    } else if arm {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    };

    let deno_target = if cfg!(target_os = "macos") {
        if arm { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" }
    } else if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else if arm {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let deno_file = format!("deno-{deno_target}.zip");

    let mut list = vec![
        Asset {
            component: "yt-dlp",
            url: format!("{ytdlp_base}/{ytdlp_file}"),
            checksum: Checksum::File(format!("{ytdlp_base}/SHA2-256SUMS")),
            sums_name: ytdlp_file.into(),
            unpack: Unpack::Raw,
            binaries: &["yt-dlp"],
        },
        Asset {
            component: "deno",
            url: format!("{deno_base}/{deno_file}"),
            checksum: Checksum::File(format!("{deno_base}/{deno_file}.sha256sum")),
            sums_name: deno_file.clone(),
            unpack: Unpack::Zip,
            binaries: &["deno"],
        },
    ];

    if include_ffmpeg {
        if cfg!(target_os = "macos") {
            // Pinned static builds hosted on GitHub; checksums are hard-coded.
            let base = "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1";
            let builds: [(&str, &'static [&'static str], &str); 2] = if arm {
                [
                    ("ffmpeg-darwin-arm64.gz", &["ffmpeg"], "8923876afa8db5585022d7860ec7e589af192f441c56793971276d450ed3bbfa"),
                    ("ffprobe-darwin-arm64.gz", &["ffprobe"], "d986a8ec7b030899fe66a8a288ed809a3543338705a3ce178cfb85869c5d80be"),
                ]
            } else {
                [
                    ("ffmpeg-darwin-x64.gz", &["ffmpeg"], "929b375c1182d956c51f7ac25e0b2b0411fb01f6f407aa15c9758efeb4242106"),
                    ("ffprobe-darwin-x64.gz", &["ffprobe"], "d4da574d6e2e197bd259b47d69cf262df9e312af24ad960444f6d806d3d4c186"),
                ]
            };
            for (file, binaries, sha) in builds {
                list.push(Asset {
                    component: "ffmpeg",
                    url: format!("{base}/{file}"),
                    checksum: Checksum::Pinned(sha),
                    sums_name: file.into(),
                    unpack: Unpack::Gz,
                    binaries,
                });
            }
        } else {
            let (file, unpack) = if cfg!(windows) {
                ("ffmpeg-master-latest-win64-gpl.zip", Unpack::Zip)
            } else if arm {
                ("ffmpeg-master-latest-linuxarm64-gpl.tar.xz", Unpack::TarXz)
            } else {
                ("ffmpeg-master-latest-linux64-gpl.tar.xz", Unpack::TarXz)
            };
            list.push(Asset {
                component: "ffmpeg",
                url: format!("{ffmpeg_base}/{file}"),
                checksum: Checksum::File(format!("{ffmpeg_base}/checksums.sha256")),
                sums_name: file.into(),
                unpack,
                binaries: &["ffmpeg", "ffprobe"],
            });
        }
    }
    list
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub ready: bool,
    pub missing: Vec<String>,
    pub dir: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineProgress<'a> {
    component: &'a str,
    phase: &'a str,
    downloaded: u64,
    total: Option<u64>,
}

pub struct Engine {
    dir: PathBuf,
    /// Downloads hold a read lock while spawning yt-dlp; installs and updates
    /// take the write lock so binaries are never swapped mid-spawn.
    pub lock: RwLock<()>,
}

impl Engine {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: RwLock::new(()),
        }
    }

    fn bin(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}{EXE}"))
    }

    pub fn ytdlp(&self) -> PathBuf {
        self.bin("yt-dlp")
    }

    pub fn deno(&self) -> PathBuf {
        self.bin("deno")
    }

    /// Directory with ffmpeg/ffprobe, or `None` to let yt-dlp use PATH.
    pub fn ffmpeg_location(&self) -> Option<PathBuf> {
        if self.uses_system_ffmpeg() {
            None
        } else {
            Some(self.dir.clone())
        }
    }

    /// On Linux a distro ffmpeg is usually installed already; prefer it.
    fn uses_system_ffmpeg(&self) -> bool {
        cfg!(target_os = "linux")
            && !self.bin("ffmpeg").is_file()
            && crate::proc::which("ffmpeg").is_some()
            && crate::proc::which("ffprobe").is_some()
    }

    pub fn status(&self) -> EngineStatus {
        let mut missing = Vec::new();
        if !self.ytdlp().is_file() {
            missing.push("yt-dlp".to_string());
        }
        if !self.deno().is_file() {
            missing.push("deno".to_string());
        }
        if !self.uses_system_ffmpeg()
            && !(self.bin("ffmpeg").is_file() && self.bin("ffprobe").is_file())
        {
            missing.push("ffmpeg".to_string());
        }
        EngineStatus {
            ready: missing.is_empty(),
            missing,
            dir: self.dir.to_string_lossy().into_owned(),
        }
    }

    /// Installs missing components (or all of them when `force` is set).
    pub async fn install(&self, app: &AppHandle, force: bool) -> Result<(), String> {
        let _guard = self.lock.write().await;
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        remove_temp_files(&self.dir);
        let status = self.status();
        let client = http_client()?;
        let include_ffmpeg = force || status.missing.iter().any(|m| m == "ffmpeg");

        for asset in assets(include_ffmpeg) {
            if !force && !status.missing.iter().any(|m| m == asset.component) {
                continue;
            }
            self.install_asset(app, &client, &asset)
                .await
                .map_err(|e| format!("{}: {e}", asset.component))?;
        }
        Ok(())
    }

    async fn install_asset(
        &self,
        app: &AppHandle,
        client: &reqwest::Client,
        asset: &Asset,
    ) -> Result<(), String> {
        let tmp = self.dir.join(format!(".{}.download", asset.sums_name));
        let hash = match download(app, client, asset.component, &asset.url, &tmp).await {
            Ok(hash) => hash,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        };

        emit(app, asset.component, "verify", 0, None);
        let expected = match &asset.checksum {
            Checksum::Pinned(sha) => sha.to_string(),
            Checksum::File(url) => {
                let sums = client
                    .get(url)
                    .send()
                    .await
                    .and_then(|r| r.error_for_status())
                    .map_err(|e| e.to_string())?
                    .text()
                    .await
                    .map_err(|e| e.to_string())?;
                find_hash(&sums, &asset.sums_name)
                    .ok_or_else(|| format!("checksum for {} not found", asset.sums_name))?
            }
        };
        if !expected.eq_ignore_ascii_case(&hash) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("checksum mismatch for {}", asset.sums_name));
        }

        emit(app, asset.component, "extract", 0, None);
        let dir = self.dir.clone();
        let unpack = asset.unpack;
        let binaries = asset.binaries;
        let tmp_clone = tmp.clone();
        tokio::task::spawn_blocking(move || unpack_asset(&tmp_clone, &dir, unpack, binaries))
            .await
            .map_err(|e| e.to_string())??;
        let _ = fs::remove_file(&tmp);
        emit(app, asset.component, "done", 0, None);
        Ok(())
    }

    /// Runs `yt-dlp -U`. Returns yt-dlp's output.
    pub async fn update_ytdlp(&self) -> Result<String, String> {
        let _guard = self.lock.write().await;
        let output = crate::proc::command(&self.ytdlp())
            .arg("-U")
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if output.status.success() {
            Ok(text.trim().to_string())
        } else {
            Err(text.trim().to_string())
        }
    }

    pub async fn ytdlp_version(&self) -> Option<String> {
        let output = crate::proc::command(&self.ytdlp())
            .arg("--version")
            .output()
            .await
            .ok()?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// Removes leftovers of interrupted installs (`.name.download`, `.name.tmp`).
fn remove_temp_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && (name.ends_with(".download") || name.ends_with(".tmp")) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("EasyYTubeDownloader/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        // Fail instead of hanging forever on a stalled connection.
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

fn emit(app: &AppHandle, component: &str, phase: &str, downloaded: u64, total: Option<u64>) {
    let _ = app.emit(
        "engine-progress",
        EngineProgress {
            component,
            phase,
            downloaded,
            total,
        },
    );
}

/// Streams `url` into `dest`, returning its SHA-256.
async fn download(
    app: &AppHandle,
    client: &reqwest::Client,
    component: &str,
    url: &str,
    dest: &Path,
) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut last_emit = std::time::Instant::now();
    let mut stream = resp.bytes_stream();

    emit(app, component, "download", 0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() > Duration::from_millis(150) {
            emit(app, component, "download", downloaded, total);
            last_emit = std::time::Instant::now();
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    emit(app, component, "download", downloaded, total);
    Ok(hex::encode(hasher.finalize()))
}

/// Finds the SHA-256 for `name` in a checksum file. Handles both
/// `<hash>  <name>` lists and single-hash files (including PowerShell output).
fn find_hash(text: &str, name: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?i)\b[0-9a-f]{64}\b").unwrap();
    for line in text.lines() {
        let named = line
            .split_whitespace()
            .any(|tok| tok.trim_start_matches('*').rsplit(['/', '\\']).next() == Some(name));
        if named {
            if let Some(m) = re.find(line) {
                return Some(m.as_str().to_lowercase());
            }
        }
    }
    let all: Vec<_> = re.find_iter(text).collect();
    if all.len() == 1 {
        return Some(all[0].as_str().to_lowercase());
    }
    None
}

fn unpack_asset(
    archive: &Path,
    dir: &Path,
    unpack: Unpack,
    binaries: &[&str],
) -> Result<(), String> {
    let wanted = |path: &str| -> Option<String> {
        let base = path.rsplit(['/', '\\']).next()?;
        binaries
            .iter()
            .map(|b| format!("{b}{EXE}"))
            .find(|b| b == base)
    };
    let mut found = Vec::new();

    match unpack {
        Unpack::Raw => {
            let name = format!("{}{EXE}", binaries[0]);
            place(&mut fs::File::open(archive).map_err(|e| e.to_string())?, dir, &name)?;
            found.push(name);
        }
        Unpack::Gz => {
            let file = fs::File::open(archive).map_err(|e| e.to_string())?;
            let name = format!("{}{EXE}", binaries[0]);
            place(&mut flate2::read::GzDecoder::new(file), dir, &name)?;
            found.push(name);
        }
        Unpack::Zip => {
            let file = fs::File::open(archive).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
                if !entry.is_file() {
                    continue;
                }
                if let Some(name) = wanted(entry.name()) {
                    place(&mut entry, dir, &name)?;
                    found.push(name);
                }
            }
        }
        Unpack::TarXz => {
            let file = fs::File::open(archive).map_err(|e| e.to_string())?;
            let mut tar = tar::Archive::new(xz2::read::XzDecoder::new(file));
            for entry in tar.entries().map_err(|e| e.to_string())? {
                let mut entry = entry.map_err(|e| e.to_string())?;
                if !entry.header().entry_type().is_file() {
                    continue;
                }
                let path = entry.path().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
                if let Some(name) = wanted(&path) {
                    place(&mut entry, dir, &name)?;
                    found.push(name);
                }
            }
        }
    }

    for b in binaries {
        let name = format!("{b}{EXE}");
        if !found.contains(&name) {
            return Err(format!("{name} not found in archive"));
        }
    }
    Ok(())
}

/// Writes an executable atomically (temp file + rename).
fn place(reader: &mut dyn Read, dir: &Path, name: &str) -> Result<(), String> {
    let tmp = dir.join(format!(".{name}.tmp"));
    let dest = dir.join(name);
    {
        let mut out = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        io::copy(reader, &mut out).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::find_hash;

    const H: &str = "5cd46d6268f6f78f5d88bdc7159d20bd44cdaa4b3303474839f87ec6fe7ae25c";

    #[test]
    fn hash_from_list() {
        let text = format!("{}  yt-dlp\n{H}  yt-dlp_macos\n", "a".repeat(64));
        assert_eq!(find_hash(&text, "yt-dlp_macos").as_deref(), Some(H));
        assert_eq!(find_hash(&text, "yt-dlp").unwrap(), "a".repeat(64));
    }

    #[test]
    fn hash_single() {
        assert_eq!(find_hash(&format!("{H}\n"), "x.zip").as_deref(), Some(H));
        let ps = format!("Algorithm Hash Path\n--------- ---- ----\nSHA256 {} C:\\a\\deno.zip", H.to_uppercase());
        assert_eq!(find_hash(&ps, "deno.zip").as_deref(), Some(H));
    }
}
