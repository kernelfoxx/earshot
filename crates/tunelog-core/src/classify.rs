//! Decides which service (if any) a media session belongs to.
//!
//! Desktop apps are identified by their source app id. Browsers expose no
//! page URL through the OS media controls, so for them we combine:
//! 1. the artwork host, when the backend passes the original URL;
//! 2. the artwork shape: YouTube videos have 16:9 thumbnails, while
//!    YouTube Music and SoundCloud covers are square;
//! 3. the album field: YouTube Music fills it for songs, SoundCloud and
//!    plain YouTube leave it empty.
//!
//! Anything that cannot be decided is [`Classification::Unclassified`]: it is
//! stored but excluded from stats until the user assigns it.

use crate::media::MediaSnapshot;
use crate::model::Service;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Music(Service),
    /// Probably music in a browser, but the service is unknown.
    Unclassified,
    /// Not tracked at all: video, other players, system sounds.
    Ignored,
}

const BROWSERS: &[&str] = &[
    "chrome",
    "chromium",
    "msedge",
    "microsoft-edge",
    "firefox",
    // Firefox's AppUserModelID on Windows.
    "308046b0af4a39cb",
    "brave",
    "opera",
    "vivaldi",
    "yandex",
    "librewolf",
    "floorp",
    "waterfox",
];

pub fn classify(s: &MediaSnapshot) -> Classification {
    let app = s.source_app.to_ascii_lowercase();

    if app.contains("spotify") {
        return Classification::Music(Service::Spotify);
    }
    // Unofficial YouTube Music desktop clients (th-ch/youtube-music and forks).
    if app.contains("youtube-music") || app.contains("youtubemusic") {
        return Classification::Music(Service::YoutubeMusic);
    }
    if !BROWSERS.iter().any(|b| app.contains(b)) {
        return Classification::Ignored;
    }
    if s.title.trim().is_empty() {
        return Classification::Ignored;
    }

    if let Some(host) = s.art_url.as_deref().and_then(url_host) {
        if host.ends_with("sndcdn.com") {
            return Classification::Music(Service::Soundcloud);
        }
        if host.ends_with("googleusercontent.com") || host.ends_with("ggpht.com") {
            return Classification::Music(Service::YoutubeMusic);
        }
        if host.ends_with("ytimg.com") {
            return Classification::Ignored;
        }
        if host.ends_with("scdn.co") || host.ends_with("spotifycdn.com") {
            return Classification::Music(Service::Spotify);
        }
    }

    let has_album = s.album.as_deref().is_some_and(|a| !a.trim().is_empty());
    match s.art_size.map(aspect) {
        Some(Aspect::Wide) => Classification::Ignored,
        Some(Aspect::Square) if has_album => Classification::Music(Service::YoutubeMusic),
        Some(Aspect::Square) => Classification::Music(Service::Soundcloud),
        Some(Aspect::Other) | None if has_album => Classification::Music(Service::YoutubeMusic),
        _ => Classification::Unclassified,
    }
}

enum Aspect {
    Square,
    Wide,
    Other,
}

fn aspect((w, h): (u32, u32)) -> Aspect {
    if w == 0 || h == 0 {
        return Aspect::Other;
    }
    let r = w as f64 / h as f64;
    if (0.9..=1.1).contains(&r) {
        Aspect::Square
    } else if r >= 1.25 {
        Aspect::Wide
    } else {
        Aspect::Other
    }
}

fn url_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::PlaybackStatus;

    fn snap(app: &str) -> MediaSnapshot {
        MediaSnapshot {
            session_id: app.into(),
            source_app: app.into(),
            status: PlaybackStatus::Playing,
            title: "Song".into(),
            artist: "Artist".into(),
            album: None,
            art_url: None,
            art_size: None,
            duration_ms: None,
            position_ms: None,
        }
    }

    use Classification::*;
    use Service::*;

    #[test]
    fn desktop_apps() {
        assert_eq!(classify(&snap("Spotify.exe")), Music(Spotify));
        assert_eq!(
            classify(&snap("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify")),
            Music(Spotify)
        );
        assert_eq!(
            classify(&snap("org.mpris.MediaPlayer2.spotify")),
            Music(Spotify)
        );
        assert_eq!(
            classify(&snap("com.github.th-ch.youtube-music")),
            Music(YoutubeMusic)
        );
        assert_eq!(classify(&snap("vlc.exe")), Ignored);
        assert_eq!(classify(&snap("org.mpris.MediaPlayer2.vlc")), Ignored);
    }

    #[test]
    fn browser_by_artwork_host() {
        let mut s = snap("org.mpris.MediaPlayer2.firefox.instance_1_42");
        s.art_url = Some("https://i1.sndcdn.com/artworks-abc-t500x500.jpg".into());
        assert_eq!(classify(&s), Music(Soundcloud));
        s.art_url = Some("https://lh3.googleusercontent.com/abc=w544-h544".into());
        assert_eq!(classify(&s), Music(YoutubeMusic));
        s.art_url = Some("https://i.ytimg.com/vi/abc/maxresdefault.jpg".into());
        assert_eq!(classify(&s), Ignored);
        s.art_url = Some("https://i.scdn.co/image/abc".into());
        assert_eq!(classify(&s), Music(Spotify));
    }

    #[test]
    fn browser_by_artwork_shape_and_album() {
        let mut s = snap("Chrome");
        s.art_size = Some((1280, 720));
        assert_eq!(classify(&s), Ignored, "16:9 thumbnail is a YouTube video");
        s.art_size = Some((544, 544));
        s.album = Some("Random Access Memories".into());
        assert_eq!(classify(&s), Music(YoutubeMusic));
        s.album = None;
        assert_eq!(classify(&s), Music(Soundcloud));
    }

    #[test]
    fn browser_without_artwork() {
        let mut s = snap("MSEdge");
        assert_eq!(classify(&s), Unclassified);
        s.album = Some("Album".into());
        assert_eq!(classify(&s), Music(YoutubeMusic));
        // Local Chromium artwork copy gives no host, shape decides.
        s.art_url = Some("file:///tmp/.org.chromium.Chromium.abc".into());
        s.art_size = Some((1280, 720));
        assert_eq!(classify(&s), Ignored);
    }

    #[test]
    fn empty_title_is_ignored() {
        let mut s = snap("308046B0AF4A39CB");
        s.title = " ".into();
        assert_eq!(classify(&s), Ignored);
    }

    #[test]
    fn host_parsing() {
        assert_eq!(url_host("https://a.b.com/x?y"), Some("a.b.com"));
        assert_eq!(url_host("https://u@a.com:8080/"), Some("a.com"));
        assert_eq!(url_host("file:///tmp/x"), None);
        assert_eq!(url_host("nonsense"), None);
    }
}
