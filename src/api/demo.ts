// Sample data for running the UI in a plain browser (`npm run dev`),
// used for development and README screenshots.
import type { Overview, Play } from "./types";

const H = 3_600_000;
const M = 60_000;

function totals(spotify: number, youtube_music: number, soundcloud: number, unclassified = 0) {
  return {
    spotify,
    youtube_music,
    soundcloud,
    unclassified,
    total: spotify + youtube_music + soundcloud,
  };
}

export function demoOverview(): Overview {
  return {
    backend: "demo",
    tracking_error: null,
    periods: [
      { period: "today", totals: totals(1 * H + 12 * M, 25 * M, 8 * M, 4 * M) },
      { period: "week", totals: totals(9 * H + 40 * M, 4 * H + 5 * M, 1 * H + 50 * M, 22 * M) },
      { period: "month", totals: totals(38 * H + 15 * M, 17 * H + 2 * M, 6 * H + 31 * M, 1 * H) },
      { period: "year", totals: totals(402 * H, 188 * H + 30 * M, 71 * H + 10 * M, 9 * H) },
      { period: "all", totals: totals(1290 * H, 512 * H, 233 * H, 9 * H) },
    ],
  };
}

export function demoNowPlaying(): Play {
  const now = Date.now();
  return {
    live_key: "demo",
    service: "spotify",
    title: "Instant Crush",
    artist: "Daft Punk, Julian Casablancas",
    album: "Random Access Memories",
    source_app: "Spotify.exe",
    duration_ms: 337_000,
    started_at: now - 95_000,
    ended_at: now,
    played_ms: 95_000,
  };
}
