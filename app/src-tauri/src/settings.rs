use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};

use crate::downloader::PRESETS;

pub const COOKIE_BROWSERS: &[&str] = &[
    "", "chrome", "firefox", "safari", "edge", "brave", "opera", "vivaldi", "chromium",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: String,
    pub preset: String,
    /// "auto", "en" or "ru"
    pub language: String,
    pub max_parallel: usize,
    pub playlist: bool,
    /// Browser to borrow YouTube cookies from (`--cookies-from-browser`), "" = off.
    pub cookies_browser: String,
    /// Proxy URL for yt-dlp; "" = use the system proxy, if any.
    pub proxy: String,
    /// Unix seconds of the last successful `yt-dlp -U`.
    pub last_engine_update: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: String::new(),
            preset: "best".into(),
            language: "auto".into(),
            max_parallel: 2,
            playlist: false,
            cookies_browser: String::new(),
            proxy: String::new(),
            last_engine_update: 0,
        }
    }
}

impl Settings {
    fn sanitize(&mut self, default_dir: &str) {
        if self.download_dir.trim().is_empty() {
            self.download_dir = default_dir.to_string();
        }
        if !PRESETS.contains(&self.preset.as_str()) {
            self.preset = "best".into();
        }
        if !["auto", "en", "ru"].contains(&self.language.as_str()) {
            self.language = "auto".into();
        }
        self.max_parallel = self.max_parallel.clamp(1, 5);
        if !COOKIE_BROWSERS.contains(&self.cookies_browser.as_str()) {
            self.cookies_browser.clear();
        }
        self.proxy = self.proxy.trim().to_string();
        if !self.proxy.is_empty() && !crate::system::is_valid_proxy(&self.proxy) {
            self.proxy.clear();
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    default_dir: String,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf, default_dir: String) -> Self {
        let mut settings: Settings = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        settings.sanitize(&default_dir);
        Self {
            path,
            default_dir,
            inner: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.inner.lock().unwrap().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut guard = self.inner.lock().unwrap();
        f(&mut guard);
        guard.sanitize(&self.default_dir);
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&*guard).map_err(|e| e.to_string())?;
        fs::write(&self.path, json).map_err(|e| e.to_string())?;
        Ok(guard.clone())
    }
}
