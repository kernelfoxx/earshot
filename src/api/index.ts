import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "./types";

/** True when running inside the Tauri webview (false in a plain browser / demo mode). */
export const inTauri = "__TAURI_INTERNALS__" in window;

export async function getAppInfo(): Promise<AppInfo> {
  if (!inTauri) return { version: "dev", platform: "browser" };
  return invoke<AppInfo>("app_info");
}
