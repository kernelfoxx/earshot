import { useEffect, useState } from "react";
import { getNowPlaying, onNowPlaying } from "../../api";
import { SERVICES, type Play } from "../../api/types";
import { formatClock } from "./format";

export function NowPlaying() {
  const [play, setPlay] = useState<Play | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let alive = true;
    void getNowPlaying().then((p) => alive && setPlay(p));
    const unlisten = onNowPlaying((p) => setPlay(p));
    // Re-render every second so the elapsed time ticks.
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      alive = false;
      clearInterval(timer);
      void unlisten.then((f) => f());
    };
  }, []);

  if (!play) {
    return (
      <div className="card now-playing idle">
        <span className="muted">Nothing is playing</span>
      </div>
    );
  }

  const service = SERVICES.find((s) => s.id === play.service);
  // The snapshot was taken at `ended_at`; extrapolate while it keeps playing.
  const elapsed = play.played_ms + Math.max(0, now - play.ended_at);

  return (
    <div className="card now-playing">
      <div className="eq" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      <div className="np-text">
        <div className="np-title">{play.title}</div>
        <div className="muted">
          {play.artist}
          {play.album ? ` · ${play.album}` : ""}
        </div>
      </div>
      <div className="np-meta">
        <span className="badge" style={{ color: service?.color }}>
          {service?.name ?? "Unclassified"}
        </span>
        <span className="muted tabular">{formatClock(elapsed)}</span>
      </div>
    </div>
  );
}
