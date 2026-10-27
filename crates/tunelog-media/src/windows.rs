//! Windows backend: Global System Media Transport Controls (GSMTC).
//! Requires Windows 10 1809 or newer.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use windows::Foundation::TypedEventHandler;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Properties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

use crate::{ChangeNotifier, MediaError, MediaSnapshot, MediaSource, PlaybackStatus, Result};

/// Give up reading a thumbnail after this many failed attempts per track.
const MAX_ART_ATTEMPTS: u8 = 3;

type Notify = Arc<dyn Fn() + Send + Sync>;

pub struct Gsmtc {
    manager: Manager,
    notify: Option<Notify>,
    /// Sessions we registered change handlers on, by AUMID. Kept alive so
    /// the handlers stay registered.
    watched: HashMap<String, Session>,
    /// Thumbnail size per `aumid|title|artist`: `Ok(size)` or failed attempts.
    art: HashMap<String, std::result::Result<(u32, u32), u8>>,
}

impl Gsmtc {
    pub fn new() -> Result<Self> {
        // The tracker thread needs a WinRT apartment; an "already
        // initialized" result is fine.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let manager = Manager::RequestAsync()
            .and_then(|op| op.get())
            .map_err(err)?;
        Ok(Gsmtc {
            manager,
            notify: None,
            watched: HashMap::new(),
            art: HashMap::new(),
        })
    }

    fn watch(&mut self, aumid: &str, session: &Session) {
        let Some(notify) = &self.notify else { return };
        if self.watched.contains_key(aumid) {
            return;
        }
        let n1 = notify.clone();
        let n2 = notify.clone();
        let n3 = notify.clone();
        let _ = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            n1();
            Ok(())
        }));
        let _ = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            n2();
            Ok(())
        }));
        let _ = session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            n3();
            Ok(())
        }));
        self.watched.insert(aumid.to_owned(), session.clone());
    }

    fn snapshot(&mut self, session: &Session) -> windows::core::Result<MediaSnapshot> {
        let aumid = session.SourceAppUserModelId()?.to_string();
        let props = session.TryGetMediaPropertiesAsync()?.get()?;
        let title = props.Title()?.to_string();
        let artist = props.Artist()?.to_string();
        let album = props.AlbumTitle()?.to_string();

        let status = match session.GetPlaybackInfo()?.PlaybackStatus()? {
            Status::Playing => PlaybackStatus::Playing,
            Status::Paused => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        };

        let timeline = session.GetTimelineProperties()?;
        let to_ms = |ticks: i64| u64::try_from(ticks / 10_000).ok();
        let end = timeline.EndTime()?.Duration;
        let position = timeline.Position()?.Duration;

        let art_size = if title.is_empty() {
            None
        } else {
            self.art_size(&format!("{aumid}|{title}|{artist}"), &props)
        };
        self.watch(&aumid, session);

        Ok(MediaSnapshot {
            session_id: aumid.clone(),
            source_app: aumid,
            status,
            title,
            artist,
            album: (!album.is_empty()).then_some(album),
            art_url: None,
            art_size,
            duration_ms: if end > 0 { to_ms(end) } else { None },
            position_ms: if end > 0 { to_ms(position) } else { None },
        })
    }

    /// Size of the session thumbnail, decoded once per track.
    fn art_size(&mut self, key: &str, props: &Properties) -> Option<(u32, u32)> {
        match self.art.get(key) {
            Some(Ok(size)) => return Some(*size),
            Some(Err(attempts)) if *attempts >= MAX_ART_ATTEMPTS => return None,
            _ => {}
        }
        let read = || -> windows::core::Result<(u32, u32)> {
            let stream = props.Thumbnail()?.OpenReadAsync()?.get()?;
            let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;
            Ok((decoder.PixelWidth()?, decoder.PixelHeight()?))
        };
        match read() {
            Ok(size) => {
                self.art.insert(key.to_owned(), Ok(size));
                Some(size)
            }
            Err(_) => {
                let attempts = match self.art.get(key) {
                    Some(Err(n)) => n + 1,
                    _ => 1,
                };
                self.art.insert(key.to_owned(), Err(attempts));
                None
            }
        }
    }
}

impl MediaSource for Gsmtc {
    fn name(&self) -> &'static str {
        "gsmtc"
    }

    fn subscribe(&mut self, notify: ChangeNotifier) -> Result<()> {
        let notify: Notify = Arc::from(notify);
        let n = notify.clone();
        self.manager
            .SessionsChanged(&TypedEventHandler::new(move |_, _| {
                n();
                Ok(())
            }))
            .map_err(err)?;
        self.notify = Some(notify);
        Ok(())
    }

    fn sessions(&mut self) -> Result<Vec<MediaSnapshot>> {
        let sessions = self.manager.GetSessions().map_err(err)?;
        let mut out = Vec::new();
        for session in &sessions {
            match self.snapshot(&session) {
                Ok(s) => out.push(s),
                Err(e) => log::debug!("skipping GSMTC session: {e}"),
            }
        }
        let alive: HashSet<&str> = out.iter().map(|s| s.session_id.as_str()).collect();
        self.watched.retain(|k, _| alive.contains(k.as_str()));
        self.art
            .retain(|k, _| alive.contains(k.split('|').next().unwrap_or_default()));
        Ok(out)
    }
}

fn err(e: impl std::fmt::Display) -> MediaError {
    MediaError::Backend(e.to_string())
}
