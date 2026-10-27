# Roadmap

TuneLog tracks listening time across Spotify, YouTube Music and SoundCloud
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
  In-progress plays are checkpointed every 60 s, so a crash loses at most a
  minute. A gap of more than 60 s between polls (sleep/hibernate) is not
  counted.
- **Low overhead:** the tracker reacts to OS change events (GSMTC events,
  MPRIS `PropertiesChanged`/`NameOwnerChanged`) and polls every 5 s as a
  safety net. Closing the window destroys the webview; the tray recreates it.
- **Service detection** (no browser extension, see `crates/tunelog-core/src/classify.rs`):
  1. Desktop apps by source app: Spotify (`Spotify.exe`, MPRIS `spotify`),
     unofficial YouTube Music clients (`youtube-music`). Other players are ignored.
  2. Browsers, by artwork host when the backend exposes the original URL
     (Firefox on Linux): `i1.sndcdn.com` → SoundCloud,
     `lh3.googleusercontent.com` → YouTube Music, `i.ytimg.com` → YouTube video
     (ignored), `i.scdn.co` → Spotify web player.
  3. Browsers, by artwork shape (thumbnail decoded on Windows, local artwork
     file on Chromium/Linux): 16:9 → YouTube video (ignored); square with an
     album → YouTube Music; square without an album → SoundCloud.
  4. Album present but no artwork info → YouTube Music.
  5. Anything else goes to **Unclassified**: stored, but excluded from
     totals until the user assigns it (assignment UI comes in milestone 2).

  Known limitations: the Spotify web player in Chrome/Edge on Windows looks
  like YouTube Music (square cover + album), and a non-music site with square
  artwork and no album would be counted as SoundCloud. Imported history will
  be used to correct such cases in milestone 2.
- **Imports:** Spotify `ts` is the *end* of a stream. Takeout has no duration:
  use the known track duration, else the gap to the next entry capped at
  6 min, else 3.5 min; such plays are flagged as estimated. SoundCloud's
  export contains at most the last 1,000 plays and its format is
  undocumented, so the parser auto-detects JSON/CSV and lets the user map
  columns.
- **Deduplication:** same service + normalized artist/title + overlapping
  interval (±90 s). Priority: Spotify export > live tracker > Takeout.
- **Charts:** ECharts (canvas renderer, built-in heatmap, fast on large data).
