//! System media session backends.
//!
//! Each OS gets its own [`MediaSource`] implementation; the rest of the app
//! only talks to the trait, so adding macOS later means adding one module.

pub use earshot_core::media::{MediaSnapshot, PlaybackStatus};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("media backend is not supported on this platform")]
    Unsupported,
    #[error("media backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, MediaError>;

/// Callback a backend invokes whenever something may have changed
/// (track, status, position). The tracker then calls [`MediaSource::sessions`].
pub type ChangeNotifier = Box<dyn Fn() + Send + Sync + 'static>;

/// A source of system media sessions.
///
/// Backends should be event-driven where the OS allows it and call the
/// notifier on changes; the tracker additionally polls at a low rate as a
/// safety net, so `sessions` must be cheap.
pub trait MediaSource: Send {
    /// Human-readable backend name for logs, e.g. `"gsmtc"` or `"mpris"`.
    fn name(&self) -> &'static str;

    /// Register a change notifier. Called once before the first `sessions`.
    fn subscribe(&mut self, notify: ChangeNotifier) -> Result<()>;

    /// Current state of all media sessions known to the OS.
    fn sessions(&mut self) -> Result<Vec<MediaSnapshot>>;
}

/// Backend for the current platform.
pub fn platform_source() -> Result<Box<dyn MediaSource>> {
    Err(MediaError::Unsupported)
}
