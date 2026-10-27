<p align="center">
  <img src="public/tunelog.svg" width="96" alt="TuneLog logo" />
</p>

<h1 align="center">TuneLog</h1>

<p align="center">
  🎧 Track how much music you listen to across Spotify, YouTube Music &amp; SoundCloud.<br/>
  Local-first, no accounts, no API keys.
</p>

> **Status:** early development. See the [roadmap](docs/ROADMAP.md).

## How it works

TuneLog runs quietly in your system tray and watches the operating system's
media controls (GSMTC on Windows, MPRIS on Linux) to see what is playing,
which app is playing it and whether it is actually playing. It counts only the
time a track is really playing, and a play counts once it reaches 30 seconds.

Spotify is recognized by its desktop app. YouTube Music and SoundCloud in a
browser are recognized from what the browser reports to the OS: artwork host,
artwork shape (YouTube videos have 16:9 thumbnails, music covers are square)
and album. Plain YouTube videos are not counted. Audio that can't be matched
to a service is kept aside as *unclassified* and not counted.

Coming soon: importing your history from data exports:

- **Spotify** — Extended Streaming History
- **YouTube Music** — Google Takeout `watch-history.json`
- **SoundCloud** — GDPR data export

## Privacy

- All data is stored in a local SQLite database on your computer.
- TuneLog makes no network requests: no telemetry, no analytics, no accounts.
- No official streaming service APIs are used.

## Development

Prerequisites: [Rust](https://rustup.rs), Node.js 22+, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
npm install
npm run tauri dev
```

Checks:

```sh
npm run lint && npm run typecheck && npm test
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

On Linux the tray icon needs `libayatana-appindicator3`.

### Checking what TuneLog sees

If a player is not detected, run the probe and include its output in your
bug report:

```sh
cargo run -p tunelog-media --example probe
```

### Project layout

| Path | Purpose |
|------|---------|
| `crates/tunelog-core` | Platform-independent logic: models, normalization, tracking, stats, importers |
| `crates/tunelog-media` | `MediaSource` trait and OS backends (Windows GSMTC, Linux MPRIS) |
| `src-tauri` | Tauri shell: tray, autostart, commands |
| `src` | React + TypeScript UI |

## License

[MIT](LICENSE)
