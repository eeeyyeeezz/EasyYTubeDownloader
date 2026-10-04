//! Handles `easyytd://download?url=<YouTube URL>&preset=<preset>` links opened
//! by the browser extension. Only YouTube URLs are accepted so that an
//! arbitrary website cannot make the app fetch anything else.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::downloader::PRESETS;

const ALLOWED_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkRequest {
    pub url: String,
    pub preset: Option<String>,
}

/// Links received before the UI asked for them (e.g. the one that launched the app).
#[derive(Default)]
pub struct PendingLinks(pub Mutex<Vec<LinkRequest>>);

pub fn parse(link: &str) -> Option<LinkRequest> {
    let link = url::Url::parse(link).ok()?;
    if link.scheme() != "easyytd" || link.host_str() != Some("download") {
        return None;
    }
    let mut target = None;
    let mut preset = None;
    for (k, v) in link.query_pairs() {
        match k.as_ref() {
            "url" => target = Some(v.into_owned()),
            "preset" if PRESETS.contains(&v.as_ref()) => preset = Some(v.into_owned()),
            _ => {}
        }
    }
    let target = url::Url::parse(&target?).ok()?;
    if !matches!(target.scheme(), "http" | "https") {
        return None;
    }
    let host = target.host_str()?.to_ascii_lowercase();
    if !ALLOWED_HOSTS.contains(&host.as_str()) {
        return None;
    }
    Some(LinkRequest {
        url: target.to_string(),
        preset,
    })
}

pub fn handle(app: &AppHandle, links: impl IntoIterator<Item = String>) {
    let parsed: Vec<_> = links.into_iter().filter_map(|l| parse(&l)).collect();
    if parsed.is_empty() {
        return;
    }
    app.state::<PendingLinks>().0.lock().unwrap().extend(parsed);
    // The UI reacts by calling `take_pending_links`.
    let _ = app.emit("deep-link", ());
    focus_main(app);
}

pub fn focus_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn accepts_youtube() {
        let r = parse("easyytd://download?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3Dabc&preset=mp3").unwrap();
        assert_eq!(r.url, "https://www.youtube.com/watch?v=abc");
        assert_eq!(r.preset.as_deref(), Some("mp3"));
        assert!(parse("easyytd://download/?url=https://youtu.be/abc").is_some());
    }

    #[test]
    fn rejects_others() {
        assert!(parse("easyytd://download?url=https%3A%2F%2Fexample.com%2Fx").is_none());
        assert!(parse("easyytd://download?url=https%3A%2F%2Fyoutube.com.evil.com%2F").is_none());
        assert!(parse("easyytd://download?url=file%3A%2F%2F%2Fetc%2Fpasswd").is_none());
        assert!(parse("easyytd://other?url=https%3A%2F%2Fyoutu.be%2Fabc").is_none());
        assert!(parse("https://youtu.be/abc").is_none());
        let r = parse("easyytd://download?url=https%3A%2F%2Fyoutu.be%2Fabc&preset=evil").unwrap();
        assert_eq!(r.preset, None);
    }
}
