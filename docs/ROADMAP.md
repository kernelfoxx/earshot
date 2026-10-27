# Roadmap

Earshot tracks listening time across Spotify, YouTube Music and SoundCloud
using the operating system's media controls and the user's own data exports.
No official APIs, no servers, no telemetry.

## Milestones

| # | Milestone | Scope |
|---|-----------|-------|
| 0 | Skeleton | Cargo workspace, Tauri 2 + React + Vite + TS, tray, single instance, autostart plugin, light/dark theme, CI (lint, typecheck, tests on Windows and Linux) |
| 1 | **MVP: live tracker** | `MediaSource` trait, Linux MPRIS (zbus), Windows GSMTC (windows), session state machine (Playing-only time, 30 s threshold, pause/seek/track change), service classifier, SQLite storage, background mode with low CPU, service × period overview, now playing |
| 2 | Imports | Spotify Extended Streaming History, Google Takeout (YouTube Music only, estimated durations), SoundCloud export (flexible JSON/CSV parser), deduplication against tracked plays, import UI with preview, manual assignment of unclassified plays |
| 3 | Statistics | Period navigation (previous day/week/month/year), custom date range, comparison with previous period, top artists/tracks with name normalization, stacked bar charts by day/week/month, hour × weekday heatmap |
| 4 | Recap | Slides for a month, a year or all time; PNG export (1080×1920, 1080×1080) |
| 5 | Release | Settings (threshold, classification rules, data export/wipe), release workflow (NSIS/MSI, AppImage/deb/rpm), README with screenshots and demo GIF |
| later | macOS | `MediaSource` implementation for macOS |

## Key decisions

- **Time is wall-clock time spent in `Playing`.** Browsers update GSMTC
  timeline properties rarely, so position is only used to detect seeks and
  repeats.
- **Segments, not just plays.** Every play stores the real playing intervals.
  Period totals are the overlap of segments with the period, so a track
  playing across midnight is split correctly between days.
- **30 s threshold is applied at query time.** All segments are kept; the
  threshold (default 30 s) decides which plays count and can be changed later
  without data loss. The same rule applies to imported Spotify history.
- **Storage:** UTC milliseconds plus the local UTC offset at play time for
  live plays. Weeks start on Monday, buckets use the local time zone.
- **Service detection (no browser extension):**
  1. Spotify desktop: by source app (`Spotify.exe`, MPRIS `spotify`).
  2. Artwork host when the backend exposes it (Firefox on Linux):
     `lh3.googleusercontent.com` → YouTube Music, `i1.sndcdn.com` → SoundCloud,
     `i.ytimg.com` → YouTube video (not music).
  3. Metadata heuristics: YouTube Music sets an album for songs, plain YouTube
     does not; "Official Video", "- Topic" patterns; matches against imported
     history.
  4. Anything uncertain goes to **Unclassified** and is excluded from stats
     until the user assigns it or adds a rule.
- **Imports:** Spotify `ts` is the *end* of a stream. Takeout has no duration:
  use the known track duration, else the gap to the next entry capped at
  6 min, else 3.5 min; such plays are flagged as estimated. SoundCloud's
  export contains at most the last 1,000 plays and its format is
  undocumented, so the parser auto-detects JSON/CSV and lets the user map
  columns.
- **Deduplication:** same service + normalized artist/title + overlapping
  interval (±90 s). Priority: Spotify export > live tracker > Takeout.
- **Charts:** ECharts (canvas renderer, built-in heatmap, fast on large data).
