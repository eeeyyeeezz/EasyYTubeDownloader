import { invoke } from "@tauri-apps/api/core";

export const PRESETS = ["best", "1080", "720", "mp3", "m4a"] as const;
export type Preset = (typeof PRESETS)[number];

export const REPO_URL = "https://github.com/eeeyyeeezz/EasyYTubeDownloader";
export const EXTENSION_URL = `${REPO_URL}#browser-extension`;

export const COOKIE_BROWSERS: Record<string, string> = {
  chrome: "Chrome",
  firefox: "Firefox",
  safari: "Safari",
  edge: "Edge",
  brave: "Brave",
  opera: "Opera",
  vivaldi: "Vivaldi",
  chromium: "Chromium",
};

export interface Settings {
  downloadDir: string;
  preset: Preset;
  language: "auto" | "en" | "ru";
  maxParallel: number;
  playlist: boolean;
  cookiesBrowser: string;
  proxy: string;
  lastEngineUpdate: number;
}

export interface EngineStatus {
  ready: boolean;
  missing: string[];
  dir: string;
}

export interface EngineProgress {
  component: "yt-dlp" | "ffmpeg" | "deno";
  phase: "download" | "verify" | "extract" | "done";
  downloaded: number;
  total: number | null;
}

export type JobStatus =
  | "queued"
  | "starting"
  | "downloading"
  | "processing"
  | "done"
  | "error"
  | "cancelled";

export interface JobUpdate {
  id: number;
  status: JobStatus;
  title?: string;
  thumbnail?: string;
  percent?: number;
  speed?: number;
  eta?: number;
  item?: number;
  items?: number;
  file?: string;
  error?: string;
  log?: string;
}

export interface Job extends JobUpdate {
  /** Browser whose cookies the job used, "" if none. */
  cookies: string;
  url: string;
  preset: Preset;
  dir: string;
  playlist: boolean;
}

export interface LinkRequest {
  url: string;
  preset: Preset | null;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  engineStatus: () => invoke<EngineStatus>("engine_status"),
  engineInstall: (force: boolean) => invoke<EngineStatus>("engine_install", { force }),
  engineUpdate: () => invoke<string>("engine_update"),
  engineVersion: () => invoke<string | null>("engine_version"),
  startDownload: (request: { url: string; preset: Preset; dir: string; playlist: boolean }) =>
    invoke<number>("start_download", { request }),
  cancelDownload: (id: number) => invoke<void>("cancel_download", { id }),
  takePendingLinks: () => invoke<LinkRequest[]>("take_pending_links"),
  detectBrowsers: () => invoke<string[]>("detect_browsers"),
  systemProxy: () => invoke<string | null>("system_proxy"),
};

/** Errors that signing in through browser cookies can fix. */
export const COOKIE_ERRORS = [
  "bot",
  "age",
  "members",
  "cookies-locked",
  "cookies-decrypt",
  "cookies-permission",
  "cookies-missing",
];

export function isValidProxy(text: string): boolean {
  return /^(https?|socks4a?|socks5h?):\/\/[^\s/]+/i.test(text.trim());
}

const YT = /^https?:\/\/((www|m|music)\.)?(youtube\.com|youtu\.be)\//i;

export function isYouTubeUrl(text: string): boolean {
  return YT.test(text.trim());
}

export function looksLikePlaylist(url: string): boolean {
  try {
    const u = new URL(url);
    return u.searchParams.has("list") || u.pathname.startsWith("/playlist");
  } catch {
    return false;
  }
}

export function formatBytes(n: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i > 1 ? 1 : 0)} ${units[i]}`;
}

export function formatEta(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}
