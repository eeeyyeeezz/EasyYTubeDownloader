mod deeplink;
mod downloader;
mod engine;
mod proc;
mod settings;
mod system;

use deeplink::{LinkRequest, PendingLinks};
use downloader::{DownloadRequest, Downloader};
use engine::{Engine, EngineStatus};
use settings::{Settings, SettingsStore};
use tauri::{AppHandle, Manager, State};

const DAY: u64 = 24 * 60 * 60;

#[tauri::command]
fn get_settings(store: State<SettingsStore>) -> Settings {
    store.get()
}

#[tauri::command]
fn save_settings(
    settings: Settings,
    store: State<SettingsStore>,
    downloader: State<Downloader>,
) -> Result<Settings, String> {
    let saved = store.update(|s| {
        let last = s.last_engine_update;
        *s = settings;
        s.last_engine_update = last;
    })?;
    downloader.set_limit(saved.max_parallel);
    Ok(saved)
}

#[tauri::command]
fn engine_status(engine: State<Engine>) -> EngineStatus {
    engine.status()
}

#[tauri::command]
async fn engine_install(app: AppHandle, force: bool) -> Result<EngineStatus, String> {
    let engine = app.state::<Engine>();
    engine.install(&app, force).await?;
    Ok(engine.status())
}

#[tauri::command]
async fn engine_update(app: AppHandle) -> Result<String, String> {
    let engine = app.state::<Engine>();
    let out = engine.update_ytdlp().await?;
    // Deno failing to update is not worth an error: the current one still works.
    let _ = engine.update_deno().await;
    let _ = app
        .state::<SettingsStore>()
        .update(|s| s.last_engine_update = now());
    Ok(out)
}

#[tauri::command]
async fn engine_version(engine: State<'_, Engine>) -> Result<Option<String>, String> {
    Ok(engine.ytdlp_version().await)
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    request: DownloadRequest,
    downloader: State<Downloader>,
) -> Result<u64, String> {
    downloader.start(&app, request)
}

#[tauri::command]
fn cancel_download(id: u64, downloader: State<Downloader>) {
    downloader.cancel(id);
}

#[tauri::command]
fn detect_browsers() -> Vec<&'static str> {
    system::detect_browsers()
}

#[tauri::command]
async fn system_proxy() -> Option<String> {
    system::system_proxy()
}

#[tauri::command]
fn take_pending_links(pending: State<PendingLinks>) -> Vec<LinkRequest> {
    std::mem::take(&mut *pending.0.lock().unwrap())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Keeps yt-dlp fresh: YouTube changes often break older versions.
async fn daily_update(app: AppHandle) {
    let store = app.state::<SettingsStore>();
    let engine = app.state::<Engine>();
    if !engine.status().ready || now().saturating_sub(store.get().last_engine_update) < DAY {
        return;
    }
    if engine.update_ytdlp().await.is_ok() {
        let _ = engine.update_deno().await;
        let _ = store.update(|s| s.last_engine_update = now());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        // Must come first: forwards links from a second launch to this instance.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            deeplink::focus_main(app);
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let paths = app.path();
            let default_dir = paths
                .download_dir()
                .or_else(|_| paths.home_dir())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let store = SettingsStore::load(paths.app_config_dir()?.join("settings.json"), default_dir);
            let limit = store.get().max_parallel;

            app.manage(store);
            // Local, not roaming, app data: ~150 MB of tools must not sync with
            // Windows roaming profiles. Same folder as before on macOS and Linux.
            let engine_dir = paths.app_local_data_dir()?.join("engine");
            let old_engine_dir = paths.app_data_dir()?.join("engine");
            if old_engine_dir != engine_dir && old_engine_dir.is_dir() && !engine_dir.exists() {
                if let Some(parent) = engine_dir.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(&old_engine_dir, &engine_dir);
            }
            app.manage(Engine::new(engine_dir));
            app.manage(Downloader::new(limit));
            app.manage(PendingLinks::default());

            use tauri_plugin_deep_link::DeepLinkExt;
            #[cfg(any(target_os = "linux", windows))]
            {
                // AppImage and portable runs are not registered by an installer.
                let _ = app.deep_link().register_all();
            }
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                deeplink::handle(app.handle(), urls.into_iter().map(|u| u.to_string()));
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                deeplink::handle(&handle, event.urls().into_iter().map(|u| u.to_string()));
            });

            tauri::async_runtime::spawn(daily_update(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            engine_status,
            engine_install,
            engine_update,
            engine_version,
            start_download,
            cancel_download,
            take_pending_links,
            detect_browsers,
            system_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
