//! Linux backend: MPRIS players on the D-Bus session bus.

use std::collections::HashMap;
use std::sync::Arc;

use zbus::blocking::fdo::{DBusProxy, PropertiesProxy};
use zbus::blocking::{Connection, MessageIterator};
use zbus::names::InterfaceName;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{MatchRule, message};

use crate::{ChangeNotifier, MediaError, MediaSnapshot, MediaSource, PlaybackStatus, Result};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";

pub struct Mpris {
    conn: Connection,
    /// Artwork sizes of local (Chromium) artwork files, by URL.
    art_sizes: HashMap<String, Option<(u32, u32)>>,
}

impl Mpris {
    pub fn new() -> Result<Self> {
        let conn = Connection::session().map_err(err)?;
        Ok(Mpris {
            conn,
            art_sizes: HashMap::new(),
        })
    }

    fn player(&mut self, bus_name: &str) -> Result<MediaSnapshot> {
        let props = PropertiesProxy::builder(&self.conn)
            .destination(bus_name)
            .map_err(err)?
            .path(PATH)
            .map_err(err)?
            .build()
            .map_err(err)?;
        let iface = InterfaceName::try_from(PLAYER_IFACE).map_err(err)?;
        let all = props.get_all(iface).map_err(err)?;

        let status = match all.get("PlaybackStatus").and_then(as_string).as_deref() {
            Some("Playing") => PlaybackStatus::Playing,
            Some("Paused") => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        };
        let meta: HashMap<String, OwnedValue> = all
            .get("Metadata")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| HashMap::try_from(v).ok())
            .unwrap_or_default();

        let art_url = meta.get("mpris:artUrl").and_then(as_string);
        let art_size = art_url.as_deref().and_then(|u| self.local_art_size(u));

        Ok(MediaSnapshot {
            session_id: bus_name.to_owned(),
            source_app: bus_name.trim_start_matches(PREFIX).to_owned(),
            status,
            title: meta
                .get("xesam:title")
                .and_then(as_string)
                .unwrap_or_default(),
            artist: meta
                .get("xesam:artist")
                .map(as_strings)
                .unwrap_or_default()
                .join(", "),
            album: meta.get("xesam:album").and_then(as_string),
            art_url,
            art_size,
            duration_ms: meta
                .get("mpris:length")
                .and_then(as_i64)
                .and_then(|us| u64::try_from(us / 1000).ok()),
            position_ms: all
                .get("Position")
                .and_then(as_i64)
                .and_then(|us| u64::try_from(us / 1000).ok()),
        })
    }

    /// Chromium saves artwork to a temp file and passes a `file://` URL.
    fn local_art_size(&mut self, url: &str) -> Option<(u32, u32)> {
        let path = url.strip_prefix("file://")?;
        *self.art_sizes.entry(url.to_owned()).or_insert_with(|| {
            imagesize::size(path)
                .ok()
                .map(|s| (s.width as u32, s.height as u32))
        })
    }
}

impl MediaSource for Mpris {
    fn name(&self) -> &'static str {
        "mpris"
    }

    fn subscribe(&mut self, notify: ChangeNotifier) -> Result<()> {
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::from(notify);
        // Track, status and metadata changes of any player.
        let props_changed = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .map_err(err)?
            .member("PropertiesChanged")
            .map_err(err)?
            .path(PATH)
            .map_err(err)?
            .build();
        // Players appearing and disappearing.
        let owner_changed = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .map_err(err)?
            .interface("org.freedesktop.DBus")
            .map_err(err)?
            .member("NameOwnerChanged")
            .map_err(err)?
            .arg0ns("org.mpris.MediaPlayer2")
            .map_err(err)?
            .build();
        for rule in [props_changed, owner_changed] {
            let iter = MessageIterator::for_match_rule(rule, &self.conn, Some(64)).map_err(err)?;
            let notify = notify.clone();
            std::thread::Builder::new()
                .name("mpris-signals".into())
                .spawn(move || {
                    for _ in iter {
                        notify();
                    }
                })
                .map_err(err)?;
        }
        Ok(())
    }

    fn sessions(&mut self) -> Result<Vec<MediaSnapshot>> {
        let names = DBusProxy::new(&self.conn)
            .map_err(err)?
            .list_names()
            .map_err(err)?;
        let mut out = Vec::new();
        for name in names.iter().filter(|n| n.starts_with(PREFIX)) {
            match self.player(name.as_str()) {
                Ok(s) => out.push(s),
                // Players come and go; one failing must not hide the others.
                Err(e) => log::debug!("skipping MPRIS player {name}: {e}"),
            }
        }
        // Drop cached sizes of artwork that is no longer shown.
        let shown: Vec<&str> = out.iter().filter_map(|s| s.art_url.as_deref()).collect();
        self.art_sizes.retain(|k, _| shown.contains(&k.as_str()));
        Ok(out)
    }
}

fn err(e: impl std::fmt::Display) -> MediaError {
    MediaError::Backend(e.to_string())
}

fn unwrap_variant<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
    match v {
        Value::Value(inner) => unwrap_variant(inner),
        other => other,
    }
}

fn as_string(v: &OwnedValue) -> Option<String> {
    match unwrap_variant(v) {
        Value::Str(s) => Some(s.to_string()),
        Value::ObjectPath(p) => Some(p.to_string()),
        _ => None,
    }
}

fn as_strings(v: &OwnedValue) -> Vec<String> {
    match unwrap_variant(v) {
        Value::Array(a) => a
            .iter()
            .filter_map(|x| match unwrap_variant(x) {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        Value::Str(s) => vec![s.to_string()],
        _ => Vec::new(),
    }
}

fn as_i64(v: &OwnedValue) -> Option<i64> {
    match unwrap_variant(v) {
        Value::I64(n) => Some(*n),
        Value::U64(n) => i64::try_from(*n).ok(),
        Value::I32(n) => Some((*n).into()),
        Value::U32(n) => Some((*n).into()),
        Value::F64(n) => Some(*n as i64),
        _ => None,
    }
}
