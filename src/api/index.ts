import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppInfo,
  ImportKind,
  ImportRecord,
  ImportReport,
  Overview,
  Play,
  Service,
  SoundcloudMapping,
  SoundcloudPreview,
  UnclassifiedGroup,
} from "./types";
import {
  demoImportReport,
  demoImports,
  demoNowPlaying,
  demoOverview,
  demoSoundcloudPreview,
  demoUnclassified,
} from "./demo";

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

/** Opens the system file picker. Returns `null` if cancelled. */
export async function pickFiles(extensions: string[]): Promise<string[] | null> {
  if (!inTauri) return ["demo-export.zip"];
  const picked = await open({
    multiple: true,
    directory: false,
    filters: [{ name: "Data export", extensions }],
  });
  if (!picked) return null;
  return Array.isArray(picked) ? picked : [picked];
}

export async function importFiles(
  kind: ImportKind,
  paths: string[],
  mapping?: SoundcloudMapping,
): Promise<ImportReport> {
  if (!inTauri) return demoImportReport();
  return invoke<ImportReport>("import_files", { kind, paths, mapping: mapping ?? null });
}

export async function previewSoundcloud(
  paths: string[],
  mapping?: SoundcloudMapping,
): Promise<SoundcloudPreview> {
  if (!inTauri) return demoSoundcloudPreview(mapping);
  return invoke<SoundcloudPreview>("preview_soundcloud", { paths, mapping: mapping ?? null });
}

export async function listImports(): Promise<ImportRecord[]> {
  if (!inTauri) return demoImports();
  return invoke<ImportRecord[]>("list_imports");
}

export async function deleteImport(id: number): Promise<void> {
  if (!inTauri) return;
  return invoke("delete_import", { id });
}

export async function getUnclassified(): Promise<UnclassifiedGroup[]> {
  if (!inTauri) return demoUnclassified();
  return invoke<UnclassifiedGroup[]>("get_unclassified");
}

/** `service: null` marks the artist as not music. */
export async function assignArtist(normArtist: string, service: Service | null): Promise<void> {
  if (!inTauri) return;
  return invoke("assign_artist", { normArtist, service });
}
