import { useEffect, useState } from "react";
import { getOverview, onStatsUpdated } from "../../api";
import { PERIODS, SERVICES, type Overview } from "../../api/types";
import { formatDuration } from "./format";
import { NowPlaying } from "./NowPlaying";

/** Refresh even without events so "Today" stays current across midnight. */
const REFRESH_MS = 60_000;

export function Dashboard() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      getOverview()
        .then((o) => {
          if (alive) {
            setOverview(o);
            setError(null);
          }
        })
        .catch((e: unknown) => alive && setError(String(e)));
    load();
    const timer = setInterval(load, REFRESH_MS);
    const unlisten = onStatsUpdated(load);
    return () => {
      alive = false;
      clearInterval(timer);
      void unlisten.then((f) => f());
    };
  }, []);

  const byPeriod = new Map(overview?.periods.map((p) => [p.period, p.totals]));
  const cell = (ms: number | undefined) => (ms === undefined ? "—" : formatDuration(ms));
  const unclassified = byPeriod.get("all")?.unclassified ?? 0;

  return (
    <div className="stack">
      {overview?.tracking_error && (
        <div className="banner">
          Live tracking is unavailable: {overview.tracking_error}. Imported history still works.
        </div>
      )}
      {error && <div className="banner">Failed to load stats: {error}</div>}

      <NowPlaying />

      <div className="card">
        <table className="matrix">
          <thead>
            <tr>
              <th>Service</th>
              {PERIODS.map((p) => (
                <th key={p.id}>{p.label}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {SERVICES.map((s) => (
              <tr key={s.id}>
                <td>
                  <span className="service">
                    <span className="dot" style={{ background: s.color }} />
                    {s.name}
                  </span>
                </td>
                {PERIODS.map((p) => (
                  <td key={p.id}>{cell(byPeriod.get(p.id)?.[s.id])}</td>
                ))}
              </tr>
            ))}
            <tr className="total">
              <td>All services</td>
              {PERIODS.map((p) => (
                <td key={p.id}>{cell(byPeriod.get(p.id)?.total)}</td>
              ))}
            </tr>
          </tbody>
        </table>
      </div>

      {unclassified > 0 && (
        <p className="muted small">
          {formatDuration(unclassified)} of browser audio could not be matched to a service
          and is not counted. You will be able to assign it in a later version.
        </p>
      )}
    </div>
  );
}
