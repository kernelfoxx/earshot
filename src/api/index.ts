import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo, Overview, Play } from "./types";
import { demoNowPlaying, demoOverview } from "./demo";

/** True when running inside the Tauri webview (false in a plain browser / demo mode). */
export const inTauri = "__TAURI_INTERNALS__" in window;

export async function getAppInfo(): Promise<AppInfo> {
  if (!inTauri) return { version: "dev", platform: "browser" };
  return invoke<AppInfo>("app_info");
}

export async function getOverview(): Promise<Overview> {
  if (!inTauri) return demoOverview();
  return invoke<Overview>("get_overview");
}

export async function getNowPlaying(): Promise<Play | null> {
  if (!inTauri) return demoNowPlaying();
  return invoke<Play | null>("get_now_playing");
}

/** Fires when stored listening time changed. */
export function onStatsUpdated(cb: () => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen("stats-updated", () => cb());
}

/** Fires when the current track or its playing state changed. */
export function onNowPlaying(cb: (play: Play | null) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen<Play | null>("now-playing", (e) => cb(e.payload));
}
