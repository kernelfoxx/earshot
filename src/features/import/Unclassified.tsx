import { useCallback, useEffect, useState } from "react";
import { assignArtist, getUnclassified, onStatsUpdated } from "../../api";
import { SERVICES, type Service, type UnclassifiedGroup } from "../../api/types";
import { formatDuration } from "../dashboard/format";

/** Browser audio TuneLog could not match to a service, grouped by artist. */
export function Unclassified() {
  const [groups, setGroups] = useState<UnclassifiedGroup[]>([]);

  const reload = useCallback(() => {
    void getUnclassified().then(setGroups);
  }, []);

  useEffect(() => {
    reload();
    const unlisten = onStatsUpdated(reload);
    return () => void unlisten.then((f) => f());
  }, [reload]);

  if (groups.length === 0) return null;

  const assign = (g: UnclassifiedGroup, service: Service | null) => {
    setGroups((gs) => gs.filter((x) => x.norm_artist !== g.norm_artist));
    void assignArtist(g.norm_artist, service).then(reload);
  };

  return (
    <section className="stack">
      <h2 className="section-title">Unclassified audio</h2>
      <p className="muted small">
        Browser audio that could not be matched to a service is not counted. Pick a service per
        artist; future plays by the same artist follow your choice.
      </p>
      <div className="card">
        <table className="list">
          <tbody>
            {groups.map((g) => (
              <tr key={g.norm_artist}>
                <td>
                  <div>{g.artist || <em>Unknown artist</em>}</div>
                  <div className="muted small">
                    {g.sample_titles.join(", ")} · {g.source_apps.join(", ")}
                  </div>
                </td>
                <td className="muted small nowrap">
                  {g.plays} {g.plays === 1 ? "play" : "plays"} · {formatDuration(g.total_ms)}
                </td>
                <td className="right nowrap">
                  {SERVICES.map((s) => (
                    <button
                      key={s.id}
                      className="btn subtle"
                      style={{ color: s.color }}
                      onClick={() => assign(g, s.id)}
                    >
                      {s.name}
                    </button>
                  ))}
                  <button className="btn subtle" onClick={() => assign(g, null)}>
                    Not music
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
