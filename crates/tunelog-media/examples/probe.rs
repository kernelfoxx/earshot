//! Prints what the OS media controls report and how TuneLog classifies it.
//!
//! Useful for bug reports about undetected players:
//! `cargo run -p tunelog-media --example probe`

use std::sync::mpsc::sync_channel;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tunelog_core::classify::classify;
use tunelog_core::tracker::Tracker;

fn main() {
    let mut source = tunelog_media::platform_source().expect("no media backend");
    let (tx, rx) = sync_channel::<()>(1);
    source
        .subscribe(Box::new(move || {
            let _ = tx.try_send(());
        }))
        .expect("subscribe failed");
    println!("backend: {}", source.name());

    let mut tracker = Tracker::default();
    loop {
        let _ = rx.recv_timeout(Duration::from_secs(2));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let sessions = source.sessions().expect("sessions failed");
        for s in &sessions {
            println!(
                "{:?} {:?} | {} — {} | album {:?} | art {:?} {:?} | pos {:?}/{:?} | {}",
                classify(s),
                s.status,
                s.artist,
                s.title,
                s.album,
                s.art_url,
                s.art_size,
                s.position_ms,
                s.duration_ms,
                s.source_app,
            );
        }
        let update = tracker.update(now, &sessions);
        for p in update.finished {
            println!(
                ">>> finished: {:?} {} — {} played {} s",
                p.service,
                p.artist,
                p.title,
                p.played_ms / 1000
            );
        }
    }
}
