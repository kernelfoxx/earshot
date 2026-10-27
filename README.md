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

You can also import your history from data exports:

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

### Project layout

| Path | Purpose |
|------|---------|
| `crates/tunelog-core` | Platform-independent logic: models, normalization, tracking, stats, importers |
| `crates/tunelog-media` | `MediaSource` trait and OS backends (Windows GSMTC, Linux MPRIS) |
| `src-tauri` | Tauri shell: tray, autostart, commands |
| `src` | React + TypeScript UI |

## License

[MIT](LICENSE)
