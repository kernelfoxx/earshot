use serde::{Deserialize, Serialize};

/// A music streaming service Earshot tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    Spotify,
    YoutubeMusic,
    Soundcloud,
}

impl Service {
    pub const ALL: [Service; 3] = [Service::Spotify, Service::YoutubeMusic, Service::Soundcloud];

    /// Stable identifier used in the database and the frontend.
    pub fn as_str(self) -> &'static str {
        match self {
            Service::Spotify => "spotify",
            Service::YoutubeMusic => "youtube_music",
            Service::Soundcloud => "soundcloud",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Service::Spotify => "Spotify",
            Service::YoutubeMusic => "YouTube Music",
            Service::Soundcloud => "SoundCloud",
        }
    }

    pub fn from_str_id(id: &str) -> Option<Service> {
        Service::ALL.into_iter().find(|s| s.as_str() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for s in Service::ALL {
            assert_eq!(Service::from_str_id(s.as_str()), Some(s));
        }
        assert_eq!(Service::from_str_id("tidal"), None);
    }

    #[test]
    fn serde_uses_snake_case_ids() {
        let json = serde_json::to_string(&Service::YoutubeMusic).unwrap();
        assert_eq!(json, "\"youtube_music\"");
    }
}
