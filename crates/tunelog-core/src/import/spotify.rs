//! Spotify exports.
//!
//! * Extended Streaming History (`Streaming_History_Audio_*.json`): `ts` is
//!   when the stream **ended**, `ms_played` how long it played.
//! * Account data (`StreamingHistory_music_*.json`, older
//!   `StreamingHistory*.json`): `endTime` in UTC, `msPlayed`.
//!
//! Podcast episodes, audiobooks and video entries are skipped.

use serde_json::Value;

use super::time::parse_ms;
use super::{ImportError, ImportedPlay, InputFile, Parsed};

/// Which archive entries to read.
pub fn wanted(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    file.ends_with(".json")
        && (file.starts_with("Streaming_History_Audio") || file.starts_with("StreamingHistory"))
        && !file.contains("podcast")
}

pub fn parse(files: &[InputFile]) -> Result<Parsed, ImportError> {
    let mut out = Parsed::default();
    for f in files {
        let items: Vec<Value> = serde_json::from_slice(&f.data)
            .map_err(|e| ImportError::Format(f.name.clone(), e.to_string()))?;
        for item in &items {
            match parse_item(item) {
                Some(p) => out.plays.push(p),
                None => out.skipped += 1,
            }
        }
    }
    if files.is_empty() {
        return Err(ImportError::NothingFound("Spotify streaming history"));
    }
    Ok(out)
}

fn parse_item(v: &Value) -> Option<ImportedPlay> {
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let n = |k: &str| v.get(k).and_then(Value::as_i64);

    let (title, artist, album, ended_at, played) = if v.get("ts").is_some() {
        (
            s("master_metadata_track_name")?,
            s("master_metadata_album_artist_name")?,
            s("master_metadata_album_album_name"),
            parse_ms(s("ts")?)?,
            n("ms_played")?,
        )
    } else {
        (
            s("trackName")?,
            s("artistName")?,
            None,
            parse_ms(s("endTime")?)?,
            n("msPlayed")?,
        )
    };
    if played <= 0 {
        return None;
    }
    Some(ImportedPlay {
        title: title.to_owned(),
        artist: artist.to_owned(),
        album: album.map(str::to_owned),
        started_at: ended_at - played,
        played_ms: played,
        estimated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, json: &str) -> InputFile {
        InputFile {
            name: name.into(),
            data: json.as_bytes().to_vec(),
        }
    }

    #[test]
    fn extended_history() {
        let f = file(
            "Streaming_History_Audio_2024.json",
            include_str!("../../tests/fixtures/spotify_extended.json"),
        );
        let p = parse(&[f]).unwrap();
        assert_eq!(p.plays.len(), 2);
        assert_eq!(p.skipped, 3, "podcast, audiobook and 0 ms entries");
        let a = &p.plays[0];
        assert_eq!(
            a.title,
            "Get Lucky (feat. Pharrell Williams and Nile Rodgers)"
        );
        assert_eq!(a.artist, "Daft Punk");
        assert_eq!(a.album.as_deref(), Some("Random Access Memories"));
        assert_eq!(a.played_ms, 248_413);
        // `ts` is the end of the stream.
        assert_eq!(a.ended_at(), parse_ms("2024-03-01T20:04:08Z").unwrap());
        assert!(!a.estimated);
    }

    #[test]
    fn account_data_history() {
        let f = file(
            "StreamingHistory_music_0.json",
            include_str!("../../tests/fixtures/spotify_account.json"),
        );
        let p = parse(&[f]).unwrap();
        assert_eq!(p.plays.len(), 2);
        assert_eq!(p.plays[1].artist, "Кино");
        assert_eq!(p.plays[1].ended_at(), parse_ms("2023-12-31 23:59").unwrap());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse(&[file("x.json", "{oops")]).is_err());
    }

    #[test]
    fn archive_entry_filter() {
        assert!(wanted(
            "Spotify Extended Streaming History/Streaming_History_Audio_2023.json"
        ));
        assert!(wanted("Spotify Account Data/StreamingHistory_music_0.json"));
        assert!(wanted("MyData/StreamingHistory0.json"));
        assert!(!wanted(
            "Spotify Extended Streaming History/Streaming_History_Video_2023.json"
        ));
        assert!(!wanted(
            "Spotify Account Data/StreamingHistory_podcast_0.json"
        ));
        assert!(!wanted("Spotify Account Data/Playlist1.json"));
    }
}
