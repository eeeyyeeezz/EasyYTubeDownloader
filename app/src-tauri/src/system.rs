//! Facts about the user's computer that help get past YouTube's bot check:
//! which browsers have profiles (for `--cookies-from-browser`) and which
//! proxy the system is configured to use (YouTube blocks many IPs; people
//! often reach it through a VPN or proxy app that only sets a system proxy).

use std::path::PathBuf;

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name).map(PathBuf::from).unwrap_or_default()
}

/// Browser ids (as yt-dlp names them) with a profile on this computer,
/// in the order we suggest them.
pub fn detect_browsers() -> Vec<&'static str> {
    candidates()
        .into_iter()
        .filter(|(_, paths)| paths.iter().any(|p| !p.as_os_str().is_empty() && p.exists()))
        .map(|(id, _)| id)
        .collect()
}

#[cfg(target_os = "macos")]
fn candidates() -> Vec<(&'static str, Vec<PathBuf>)> {
    let support = env_path("HOME").join("Library/Application Support");
    vec![
        ("chrome", vec![support.join("Google/Chrome")]),
        ("firefox", vec![support.join("Firefox/Profiles")]),
        ("edge", vec![support.join("Microsoft Edge")]),
        ("brave", vec![support.join("BraveSoftware/Brave-Browser")]),
        ("opera", vec![support.join("com.operasoftware.Opera")]),
        ("vivaldi", vec![support.join("Vivaldi")]),
        ("chromium", vec![support.join("Chromium")]),
        // Last: reading Safari cookies needs Full Disk Access.
        ("safari", vec![PathBuf::from("/Applications/Safari.app")]),
    ]
}

#[cfg(windows)]
fn candidates() -> Vec<(&'static str, Vec<PathBuf>)> {
    let local = env_path("LOCALAPPDATA");
    let roaming = env_path("APPDATA");
    vec![
        // First: Chromium browsers encrypt cookies in a way yt-dlp often can't read on Windows.
        ("firefox", vec![roaming.join("Mozilla/Firefox/Profiles")]),
        ("chrome", vec![local.join("Google/Chrome/User Data")]),
        ("edge", vec![local.join("Microsoft/Edge/User Data")]),
        ("brave", vec![local.join("BraveSoftware/Brave-Browser/User Data")]),
        ("opera", vec![roaming.join("Opera Software/Opera Stable")]),
        ("vivaldi", vec![local.join("Vivaldi/User Data")]),
        ("chromium", vec![local.join("Chromium/User Data")]),
    ]
}

#[cfg(not(any(target_os = "macos", windows)))]
fn candidates() -> Vec<(&'static str, Vec<PathBuf>)> {
    let home = env_path("HOME");
    let config = home.join(".config");
    vec![
        (
            "firefox",
            vec![home.join(".mozilla/firefox"), home.join("snap/firefox/common/.mozilla/firefox")],
        ),
        ("chrome", vec![config.join("google-chrome")]),
        ("chromium", vec![config.join("chromium"), home.join("snap/chromium/common/chromium")]),
        ("edge", vec![config.join("microsoft-edge")]),
        ("brave", vec![config.join("BraveSoftware/Brave-Browser")]),
        ("opera", vec![config.join("opera")]),
        ("vivaldi", vec![config.join("vivaldi")]),
    ]
}

/// The system-wide proxy as a URL yt-dlp understands, if one is configured.
/// On Linux, proxy environment variables are inherited by yt-dlp directly.
pub fn system_proxy() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("scutil").arg("--proxy").output().ok()?;
        parse_scutil(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(windows)]
    {
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
        let enabled = reg_value(key, "ProxyEnable")?;
        if !matches!(enabled.as_str(), "0x1" | "1") {
            return None;
        }
        parse_windows_proxy(&reg_value(key, "ProxyServer")?)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

#[cfg(windows)]
fn reg_value(key: &str, name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", key, "/v", name])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with(name))?;
    line.split_whitespace().nth(2).map(String::from)
}

/// Parses `scutil --proxy` output; prefers HTTPS, then SOCKS, then HTTP.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_scutil(text: &str) -> Option<String> {
    let value = |key: &str| {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once(" : ")?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    };
    for (kind, scheme) in [("HTTPS", "http"), ("SOCKS", "socks5"), ("HTTP", "http")] {
        if value(&format!("{kind}Enable")).as_deref() == Some("1") {
            let host = value(&format!("{kind}Proxy"))?;
            let port = value(&format!("{kind}Port"))?;
            return Some(format!("{scheme}://{host}:{port}"));
        }
    }
    None
}

/// Parses Windows `ProxyServer`: either `host:port` or `http=h:p;https=h:p;socks=h:p`.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_windows_proxy(server: &str) -> Option<String> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }
    if !server.contains('=') {
        return Some(if server.contains("://") {
            server.to_string()
        } else {
            format!("http://{server}")
        });
    }
    let entries: Vec<(&str, &str)> = server
        .split(';')
        .filter_map(|e| e.split_once('='))
        .collect();
    let find = |name: &str| entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v);
    if let Some(v) = find("https") {
        return Some(format!("http://{v}"));
    }
    if let Some(v) = find("socks") {
        return Some(format!("socks5://{v}"));
    }
    find("http").map(|v| format!("http://{v}"))
}

/// Accepts the proxy URL forms yt-dlp supports.
pub fn is_valid_proxy(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h")
            && u.host_str().is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scutil() {
        let text = "<dictionary> {\n  HTTPEnable : 1\n  HTTPPort : 8080\n  HTTPProxy : 10.0.0.1\n  HTTPSEnable : 1\n  HTTPSPort : 7890\n  HTTPSProxy : 127.0.0.1\n  SOCKSEnable : 0\n}";
        assert_eq!(parse_scutil(text).as_deref(), Some("http://127.0.0.1:7890"));
        let socks = "  SOCKSEnable : 1\n  SOCKSPort : 1080\n  SOCKSProxy : 127.0.0.1\n";
        assert_eq!(parse_scutil(socks).as_deref(), Some("socks5://127.0.0.1:1080"));
        assert_eq!(parse_scutil("<dictionary> {\n  HTTPEnable : 0\n}"), None);
    }

    #[test]
    fn windows_proxy() {
        assert_eq!(parse_windows_proxy("127.0.0.1:8080").as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(
            parse_windows_proxy("http=1.1.1.1:80;https=2.2.2.2:443;socks=3.3.3.3:1080").as_deref(),
            Some("http://2.2.2.2:443")
        );
        assert_eq!(parse_windows_proxy("socks=3.3.3.3:1080").as_deref(), Some("socks5://3.3.3.3:1080"));
        assert_eq!(parse_windows_proxy(""), None);
    }

    #[test]
    fn proxy_validation() {
        assert!(is_valid_proxy("socks5://127.0.0.1:1080"));
        assert!(is_valid_proxy("http://user:pass@proxy.example:3128"));
        assert!(!is_valid_proxy("127.0.0.1:1080"));
        assert!(!is_valid_proxy("ftp://x"));
    }
}
