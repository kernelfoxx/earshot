//! Platform-independent core of TuneLog.
//!
//! Nothing here depends on the OS media APIs or Tauri, so all of it is
//! unit-tested on any platform.

pub mod classify;
#[cfg(feature = "storage")]
pub mod db;
pub mod media;
pub mod model;
pub mod normalize;
pub mod period;
pub mod tracker;
