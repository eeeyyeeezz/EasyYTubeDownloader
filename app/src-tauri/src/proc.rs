//! Helpers for spawning the engine binaries without console windows and
//! for killing them together with their children (yt-dlp spawns ffmpeg, and
//! the PyInstaller one-file build runs as a bootloader + child pair).

use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Terminates the process and every process it started.
pub async fn kill_tree(pid: u32) {
    #[cfg(unix)]
    {
        let pgid = -(pid as i32);
        unsafe {
            libc::kill(pgid, libc::SIGTERM);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        unsafe {
            libc::kill(pgid, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Finds an executable in PATH.
#[allow(dead_code)]
pub fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[tokio::test]
    async fn kill_tree_stops_children() {
        // A parent shell with a background grandchild, like yt-dlp + ffmpeg.
        let mut child = command(Path::new("/bin/sh"))
            .args(["-c", "sleep 60 & echo $!; wait"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = tokio::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        tokio::io::AsyncBufReadExt::read_line(&mut out, &mut line).await.unwrap();
        let grandchild: i32 = line.trim().parse().unwrap();
        assert!(alive(grandchild));

        kill_tree(child.id().unwrap()).await;
        child.wait().await.unwrap();
        // Reaped by init once the group is gone; give the OS a moment.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!alive(grandchild), "grandchild survived kill_tree");
    }
}
