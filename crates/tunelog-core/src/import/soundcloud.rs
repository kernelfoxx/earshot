//! SoundCloud GDPR export.
//!
//! The format is undocumented and SoundCloud includes only the last 1,000
//! plays, so this parser is deliberately flexible: it accepts JSON (an
//! array, or an object containing one) or CSV, finds the timestamp, title,
//! artist, duration and URL columns by name, and lets the caller override
//! the detected mapping. Missing artists/titles are taken from track URLs
//! (`soundcloud.com/<artist>/<track>`); missing durations are estimated.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::time::parse_ms;
use super::{ImportError, ImportedPlay, InputFile, Parsed, estimate_in_order};

/// Which source column feeds which field. Column names as in the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping {
    pub time: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration: Option<String>,
    pub url: Option<String>,
}

/// What the parser found, for the column-mapping screen.
#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub columns: Vec<String>,
    pub mapping: Mapping,
    pub rows: usize,
    /// First rows as parsed with `mapping`.
    pub sample: Vec<SampleRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleRow {
    pub time: Option<i64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration_ms: Option<i64>,
}

const TIME: &[&str] = &[
    "playedat",
    "listenedat",
    "timestamp",
    "time",
    "createdat",
    "date",
    "datetime",
    "played",
];
const TITLE: &[&str] = &["tracktitle", "trackname", "title", "track", "name", "song"];
const ARTIST: &[&str] = &[
    "artist",
    "artistname",
    "trackartist",
    "username",
    "uploader",
    "creator",
    "user",
    "owner",
];
const DURATION: &[&str] = &[
    "msplayed",
    "playduration",
    "durationms",
    "duration",
    "playedms",
    "listenduration",
];
const URL: &[&str] = &[
    "trackurl",
    "permalinkurl",
    "url",
    "permalink",
    "link",
    "trackpermalink",
];

pub fn wanted(name: &str) -> bool {
    let lower = name.to_lowercase();
    (lower.ends_with(".json") || lower.ends_with(".csv"))
        && ["listen", "play", "history", "stream"]
            .iter()
            .any(|k| lower.contains(k))
}

type Row = BTreeMap<String, String>;

fn rows(files: &[InputFile]) -> Result<(Vec<String>, Vec<Row>), ImportError> {
    let mut columns: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for f in files {
        let text = String::from_utf8_lossy(&f.data);
        let text = text.trim_start_matches('\u{feff}');
        let file_rows = if text.trim_start().starts_with(['[', '{']) {
            json_rows(&f.name, text)?
        } else {
            csv_rows(&f.name, text)?
        };
        for r in &file_rows {
            for k in r.keys() {
                if !columns.contains(k) {
                    columns.push(k.clone());
                }
            }
        }
        out.extend(file_rows);
    }
    Ok((columns, out))
}

fn json_rows(name: &str, text: &str) -> Result<Vec<Row>, ImportError> {
    let v: Value =
        serde_json::from_str(text).map_err(|e| ImportError::Format(name.into(), e.to_string()))?;
    let items = find_array(&v)
        .ok_or_else(|| ImportError::Format(name.into(), "no list of plays found".into()))?;
    Ok(items
        .iter()
        .filter_map(Value::as_object)
        .map(|obj| {
            let mut row = Row::new();
            flatten("", obj, &mut row);
            row
        })
        .collect())
}

/// The largest array of objects anywhere in the document.
fn find_array(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Array(a) if a.iter().any(Value::is_object) => Some(a),
        Value::Array(a) => a.iter().filter_map(find_array).max_by_key(|a| a.len()),
        Value::Object(o) => o.values().filter_map(find_array).max_by_key(|a| a.len()),
        _ => None,
    }
}

/// Nested objects become dotted columns: `{"track": {"title": …}}` → `track.title`.
fn flatten(prefix: &str, obj: &serde_json::Map<String, Value>, out: &mut Row) {
    for (k, v) in obj {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Object(inner) => flatten(&key, inner, out),
            Value::String(s) => {
                out.insert(key, s.clone());
            }
            Value::Number(n) => {
                out.insert(key, n.to_string());
            }
            _ => {}
        }
    }
}

fn csv_rows(name: &str, text: &str) -> Result<Vec<Row>, ImportError> {
    let delimiter = b",;\t"
        .iter()
        .copied()
        .max_by_key(|d| {
            text.lines()
                .next()
                .unwrap_or("")
                .bytes()
                .filter(|b| b == d)
                .count()
        })
        .unwrap_or(b',');
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|e| ImportError::Format(name.into(), e.to_string()))?
        .clone();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| ImportError::Format(name.into(), e.to_string()))?;
        out.push(
            headers
                .iter()
                .zip(rec.iter())
                .filter(|(_, v)| !v.trim().is_empty())
                .map(|(h, v)| (h.trim().to_owned(), v.trim().to_owned()))
                .collect(),
        );
    }
    Ok(out)
}

fn key(column: &str) -> String {
    // "track.title" → "title" for matching, keeping the most specific part.
    let last = column.rsplit('.').next().unwrap_or(column);
    last.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn detect(columns: &[String]) -> Mapping {
    let find = |candidates: &[&str], taken: &[&Option<String>]| {
        candidates.iter().find_map(|cand| {
            columns
                .iter()
                .find(|c| {
                    key(c) == *cand && !taken.iter().any(|t| t.as_deref() == Some(c.as_str()))
                })
                .cloned()
        })
    };
    let time = find(TIME, &[]);
    let url = find(URL, &[&time]);
    let duration = find(DURATION, &[&time, &url]);
    let artist = find(ARTIST, &[&time, &url, &duration]);
    let title = find(TITLE, &[&time, &url, &duration, &artist]);
    Mapping {
        time,
        title,
        artist,
        duration,
        url,
    }
}

struct Extracted {
    time: Option<i64>,
    title: Option<String>,
    artist: Option<String>,
    duration_ms: Option<i64>,
}

fn extract(row: &Row, m: &Mapping) -> Extracted {
    let get = |c: &Option<String>| c.as_ref().and_then(|c| row.get(c)).map(String::as_str);
    let (url_artist, url_title) = get(&m.url).map(parse_track_url).unwrap_or_default();
    Extracted {
        time: get(&m.time).and_then(parse_ms),
        title: get(&m.title).map(str::to_owned).or(url_title),
        artist: get(&m.artist).map(str::to_owned).or(url_artist),
        duration_ms: get(&m.duration).and_then(parse_duration_ms),
    }
}

/// `https://soundcloud.com/artist-name/track-name` → ("artist-name", "track name").
fn parse_track_url(url: &str) -> (Option<String>, Option<String>) {
    let path = url.split("soundcloud.com/").nth(1).unwrap_or("");
    let mut parts = path.split(['/', '?', '#']).filter(|p| !p.is_empty());
    let artist = parts.next().map(str::to_owned);
    let title = parts.next().map(|t| t.replace('-', " "));
    (artist, title)
}

/// Milliseconds, seconds (small numbers or "s"), or "m:ss"/"h:mm:ss".
fn parse_duration_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.contains(':') {
        let mut total = 0i64;
        for part in s.split(':') {
            total = total * 60 + part.trim().parse::<i64>().ok()?;
        }
        return Some(total * 1000);
    }
    if let Some(sec) = s.strip_suffix('s') {
        return sec.trim().parse::<f64>().ok().map(|v| (v * 1000.0) as i64);
    }
    let n: f64 = s.parse().ok()?;
    // Plays longer than ~3 hours in seconds are implausible: treat as ms.
    Some(if n < 10_000.0 {
        (n * 1000.0) as i64
    } else {
        n as i64
    })
}

pub fn preview(files: &[InputFile], mapping: Option<&Mapping>) -> Result<Preview, ImportError> {
    let (columns, rows) = rows(files)?;
    let mapping = mapping.cloned().unwrap_or_else(|| detect(&columns));
    let sample = rows
        .iter()
        .take(5)
        .map(|r| {
            let e = extract(r, &mapping);
            SampleRow {
                time: e.time,
                title: e.title,
                artist: e.artist,
                duration_ms: e.duration_ms,
            }
        })
        .collect();
    Ok(Preview {
        columns,
        mapping,
        rows: rows.len(),
        sample,
    })
}

pub fn parse(
    files: &[InputFile],
    mapping: Option<&Mapping>,
    known: &dyn Fn(&str, &str) -> Option<i64>,
) -> Result<Parsed, ImportError> {
    if files.is_empty() {
        return Err(ImportError::NothingFound("SoundCloud listening history"));
    }
    let (columns, rows) = rows(files)?;
    let mapping = mapping.cloned().unwrap_or_else(|| detect(&columns));
    if mapping.time.is_none() {
        return Err(ImportError::Format(
            files[0].name.clone(),
            "could not find a timestamp column".into(),
        ));
    }

    let mut out = Parsed::default();
    let mut with_duration = Vec::new();
    let mut without_duration = Vec::new();
    for row in &rows {
        let e = extract(row, &mapping);
        let (Some(time), Some(title), Some(artist)) = (e.time, e.title, e.artist) else {
            out.skipped += 1;
            continue;
        };
        let mut play = ImportedPlay {
            title,
            artist,
            album: None,
            started_at: time,
            played_ms: 0,
            estimated: false,
        };
        match e.duration_ms.filter(|d| *d > 0) {
            Some(d) => {
                play.played_ms = d;
                with_duration.push(play);
            }
            None => without_duration.push(play),
        }
    }
    estimate_in_order(&mut without_duration, known);
    out.plays = with_duration;
    out.plays.extend(without_duration);
    out.plays.sort_by_key(|p| p.started_at);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, data: &str) -> Vec<InputFile> {
        vec![InputFile {
            name: name.into(),
            data: data.as_bytes().to_vec(),
        }]
    }

    #[test]
    fn csv_with_duration() {
        let files = input(
            "listening_history.csv",
            include_str!("../../tests/fixtures/soundcloud_listening.csv"),
        );
        let p = parse(&files, None, &|_, _| None).unwrap();
        assert_eq!(p.plays.len(), 2);
        assert_eq!(p.skipped, 1, "row without title or URL");
        assert_eq!(p.plays[0].artist, "Flume");
        assert_eq!(p.plays[0].title, "Never Be Like You");
        assert_eq!(p.plays[0].played_ms, 233_000);
        assert!(!p.plays[0].estimated);
        // Title and artist from the URL.
        assert_eq!(p.plays[1].artist, "odesza");
        assert_eq!(p.plays[1].title, "say my name");
    }

    #[test]
    fn nested_json_without_duration_is_estimated() {
        let files = input(
            "plays.json",
            include_str!("../../tests/fixtures/soundcloud_plays.json"),
        );
        let preview = preview(&files, None).unwrap();
        assert_eq!(preview.rows, 3);
        assert_eq!(preview.mapping.time.as_deref(), Some("played_at"));
        assert_eq!(preview.mapping.title.as_deref(), Some("track.title"));
        assert_eq!(
            preview.mapping.artist.as_deref(),
            Some("track.user.username")
        );

        let p = parse(&files, None, &|_, _| None).unwrap();
        assert_eq!(p.plays.len(), 3);
        assert!(p.plays.iter().all(|p| p.estimated));
        assert_eq!(p.plays[0].played_ms, 180_000, "gap to the next play");
    }

    #[test]
    fn manual_mapping_overrides_detection() {
        let csv = "When;What;Who\n2024-05-01 10:00:00;Track;Someone\n";
        let files = input("history.csv", csv);
        assert!(
            parse(&files, None, &|_, _| None).is_err(),
            "no known column names"
        );
        let m = Mapping {
            time: Some("When".into()),
            title: Some("What".into()),
            artist: Some("Who".into()),
            ..Mapping::default()
        };
        let p = parse(&files, Some(&m), &|_, _| None).unwrap();
        assert_eq!(p.plays[0].artist, "Someone");
        assert_eq!(p.plays[0].title, "Track");
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration_ms("233000"), Some(233_000));
        assert_eq!(parse_duration_ms("233"), Some(233_000));
        assert_eq!(parse_duration_ms("3:53"), Some(233_000));
        assert_eq!(parse_duration_ms("1:00:00"), Some(3_600_000));
        assert_eq!(parse_duration_ms("12.5s"), Some(12_500));
        assert_eq!(parse_duration_ms("n/a"), None);
    }
}
