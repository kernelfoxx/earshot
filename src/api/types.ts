export type Service = "spotify" | "youtube_music" | "soundcloud";

export const SERVICES: { id: Service; name: string; color: string }[] = [
  { id: "spotify", name: "Spotify", color: "var(--spotify)" },
  { id: "youtube_music", name: "YouTube Music", color: "var(--youtube-music)" },
  { id: "soundcloud", name: "SoundCloud", color: "var(--soundcloud)" },
];

export type PeriodKind = "today" | "week" | "month" | "year" | "all";

export const PERIODS: { id: PeriodKind; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "week", label: "This week" },
  { id: "month", label: "This month" },
  { id: "year", label: "This year" },
  { id: "all", label: "All time" },
];

export interface AppInfo {
  version: string;
  platform: string;
}

/** Milliseconds per service; mirrors `tunelog_core::db::ServiceTotals`. */
export interface ServiceTotals {
  spotify: number;
  youtube_music: number;
  soundcloud: number;
  unclassified: number;
  total: number;
}

export interface PeriodTotals {
  period: PeriodKind;
  totals: ServiceTotals;
}

export interface Overview {
  periods: PeriodTotals[];
  backend: string | null;
  tracking_error: string | null;
}

/** Mirrors `tunelog_core::tracker::Play`. */
export interface Play {
  live_key: string;
  service: Service | null;
  title: string;
  artist: string;
  album: string | null;
  source_app: string;
  duration_ms: number | null;
  started_at: number;
  ended_at: number;
  played_ms: number;
}
