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
