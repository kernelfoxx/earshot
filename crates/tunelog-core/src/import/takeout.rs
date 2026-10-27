//! Google Takeout `watch-history.json` (YouTube and YouTube Music).
//!
//! Only YouTube Music entries are imported. Takeout records when a track
//! started but not for how long it played, so durations are estimated
//! (see [`super::estimate`]) and flagged as such.
//!
//! Titles carry a localized prefix ("Watched …" in English). Known prefixes
//! are stripped; otherwise, for larger files, the prefix shared by all
//! titles is.

use serde_json::Value;

use super::time::parse_ms;
use super::{ImportError, ImportedPlay, InputFile, Parsed, estimate_in_order};

const KNOWN_PREFIXES: &[&str] = &["Watched "];
/// Shared-prefix detection needs enough titles to be trustworthy.
const MIN_ENTRIES_FOR_PREFIX_DETECTION: usize = 20;

pub fn wanted(name: &str) -> bool {
    name.rsplit('/').next() == Some("watch-history.json")
}

/// `known` returns a track's real duration (by raw artist and title) when
/// the tracker has seen it.
pub fn parse(
    files: &[InputFile],
    known: &dyn Fn(&str, &str) -> Option<i64>,
) -> Result<Parsed, ImportError> {
    if files.is_empty() {
        return Err(ImportError::NothingFound(
            "watch-history.json (choose JSON, not HTML, as the history format in Google Takeout)",
        ));
    }
    let mut out = Parsed::default();
    let mut raw = Vec::new();
    for f in files {
        if f.data.trim_ascii_start().starts_with(b"<") {
            return Err(ImportError::Format(
                f.name.clone(),
                "this is the HTML version; export the history as JSON in Google Takeout".into(),
            ));
        }
        let items: Vec<Value> = serde_json::from_slice(&f.data)
            .map_err(|e| ImportError::Format(f.name.clone(), e.to_string()))?;
        for item in items {
            match music_entry(&item) {
                Some(e) => raw.push(e),
                None => out.skipped += 1,
            }
        }
    }

    let prefix = title_prefix(&raw);
    for e in raw {
        let title = strip_prefix(&e.title, prefix.as_deref());
        if title.is_empty() || title.starts_with("http") {
            out.skipped += 1;
            continue;
        }
        out.plays.push(ImportedPlay {
            title: title.to_owned(),
            artist: e.artist,
            album: None,
            started_at: e.time,
            played_ms: 0,
            estimated: true,
        });
    }
    estimate_in_order(&mut out.plays, known);
    Ok(out)
}

struct Entry {
    title: String,
    artist: String,
    time: i64,
}

fn music_entry(v: &Value) -> Option<Entry> {
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    let is_music = s("header") == Some("YouTube Music")
        || s("titleUrl").is_some_and(|u| u.starts_with("https://music.youtube.com/"));
    // Ads watched on YouTube Music are listed with "From Google Ads".
    let is_ad = v.get("details").is_some();
    if !is_music || is_ad {
        return None;
    }
    // Removed or private videos have no channel.
    let channel = v.get("subtitles")?.get(0)?.get("name")?.as_str()?;
    let artist = channel.strip_suffix(" - Topic").unwrap_or(channel).trim();
    if artist.is_empty() {
        return None;
    }
    Some(Entry {
        title: s("title")?.trim().to_owned(),
        artist: artist.to_owned(),
        time: parse_ms(s("time")?)?,
    })
}

fn title_prefix(entries: &[Entry]) -> Option<String> {
    if let Some(p) = KNOWN_PREFIXES
        .iter()
        .find(|p| entries.iter().any(|e| e.title.starts_with(**p)))
    {
        return Some((*p).to_owned());
    }
    if entries.len() < MIN_ENTRIES_FOR_PREFIX_DETECTION {
        return None;
    }
    let first = &entries[0].title;
    let mut len = first.len();
    for e in &entries[1..] {
        len = first
            .char_indices()
            .zip(e.title.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, c), _)| i + c.len_utf8())
            .min(len);
    }
    // Cut back to a word boundary so a shared first letter isn't stripped.
    let common = &first[..len];
    let end = common.rfind([' ', ':'])? + 1;
    Some(first[..end].to_owned()).filter(|p| !p.trim().is_empty())
}

fn strip_prefix<'a>(title: &'a str, prefix: Option<&str>) -> &'a str {
    prefix
        .and_then(|p| title.strip_prefix(p))
        .unwrap_or(title)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::estimate::DEFAULT_MS;

    fn input(json: &str) -> Vec<InputFile> {
        vec![InputFile {
            name: "watch-history.json".into(),
            data: json.as_bytes().to_vec(),
        }]
    }

    #[test]
    fn imports_only_youtube_music() {
        let p = parse(
            &input(include_str!(
                "../../tests/fixtures/takeout_watch_history.json"
            )),
            &|_, _| None,
        )
        .unwrap();
        let got: Vec<_> = p
            .plays
            .iter()
            .map(|p| (p.artist.as_str(), p.title.as_str(), p.played_ms))
            .collect();
        assert_eq!(
            got,
            [
                // Next YouTube Music entry 3:20 later.
                ("Daft Punk", "Get Lucky", 200_000),
                // Skipped after 15 s.
                ("deadmau5", "Strobe", 15_000),
                // Last entry: default estimate.
                ("Кино", "Группа крови", DEFAULT_MS),
            ]
        );
        assert!(p.plays.iter().all(|p| p.estimated));
        // Plain YouTube video, ad, removed video.
        assert_eq!(p.skipped, 3);
    }

    #[test]
    fn known_duration_caps_estimate() {
        let p = parse(
            &input(include_str!(
                "../../tests/fixtures/takeout_watch_history.json"
            )),
            &|artist, _| (artist == "Кино").then_some(286_000),
        )
        .unwrap();
        assert_eq!(p.plays[2].played_ms, 286_000);
    }

    #[test]
    fn detects_localized_prefix() {
        const WORDS: &[&str] = &[
            "Кукушка",
            "Звезда",
            "Пачка сигарет",
            "Восьмиклассница",
            "Кончится лето",
        ];
        let entries: String = (0..25)
            .map(|i| {
                let title = format!("{} {i}", WORDS[i % WORDS.len()]);
                format!(
                    r#"{{"header":"YouTube Music","title":"Вы смотрели {title}","subtitles":[{{"name":"Кино"}}],"time":"2024-01-01T00:{i:02}:00Z"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let p = parse(&input(&format!("[{entries}]")), &|_, _| None).unwrap();
        assert_eq!(p.plays[0].title, "Кукушка 0");
        assert_eq!(p.plays[2].title, "Пачка сигарет 2");
    }

    #[test]
    fn missing_file_explains_json_format() {
        let err = parse(&[], &|_, _| None).unwrap_err().to_string();
        assert!(err.contains("JSON"), "{err}");
    }

    #[test]
    fn archive_entry_filter() {
        assert!(wanted(
            "Takeout/YouTube and YouTube Music/history/watch-history.json"
        ));
        assert!(!wanted(
            "Takeout/YouTube and YouTube Music/history/watch-history.html"
        ));
        assert!(!wanted(
            "Takeout/YouTube and YouTube Music/history/search-history.json"
        ));
    }
}
