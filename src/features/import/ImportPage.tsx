import { useCallback, useEffect, useState } from "react";
import { deleteImport, listImports, onStatsUpdated } from "../../api";
import type { ImportRecord } from "../../api/types";
import { SourceCard } from "./SourceCard";
import { SoundcloudImport } from "./SoundcloudImport";
import { Unclassified } from "./Unclassified";

const KIND_LABELS: Record<string, string> = {
  spotify_export: "Spotify",
  takeout: "YouTube Music (Takeout)",
  soundcloud_export: "SoundCloud",
};

export function ImportPage() {
  const [history, setHistory] = useState<ImportRecord[]>([]);

  const reload = useCallback(() => {
    void listImports().then(setHistory);
  }, []);

  useEffect(() => {
    reload();
    const unlisten = onStatsUpdated(reload);
    return () => void unlisten.then((f) => f());
  }, [reload]);

  return (
    <div className="stack">
      <p className="muted">
        Import your listening history from the data exports streaming services provide. Files are
        read locally and never leave your computer. Importing the same file twice is safe, and plays
        TuneLog already tracked are not counted twice.
      </p>

      <div className="source-grid">
        <SourceCard
          kind="spotify"
          title="Spotify"
          color="var(--spotify)"
          extensions={["zip", "json"]}
          steps={[
            "Open spotify.com → Account → Privacy settings.",
            "Under “Download your data”, select Extended streaming history and request it.",
            "Spotify emails a link within about 30 days.",
            "Choose the downloaded zip, or the Streaming_History_Audio_*.json files.",
          ]}
        />
        <SourceCard
          kind="takeout"
          title="YouTube Music"
          color="var(--youtube-music)"
          extensions={["zip", "json"]}
          steps={[
            "Open takeout.google.com and click “Deselect all”.",
            "Select “YouTube and YouTube Music”; under “All YouTube data included” keep only history.",
            "Under “Multiple formats”, set History to JSON.",
            "Choose the downloaded zip, or watch-history.json.",
          ]}
          note="Takeout has no play durations, so TuneLog estimates them. Only YouTube Music entries are imported."
        />
        <SoundcloudImport />
      </div>

      <Unclassified />

      {history.length > 0 && (
        <section className="stack">
          <h2 className="section-title">Import history</h2>
          <div className="card">
            <table className="list">
              <tbody>
                {history.map((h) => (
                  <tr key={h.id}>
                    <td>
                      <div>{KIND_LABELS[h.kind] ?? h.kind}</div>
                      <div className="muted small">{h.files}</div>
                    </td>
                    <td className="muted small">
                      {new Date(h.imported_at).toLocaleString()}
                    </td>
                    <td className="small">
                      {h.added.toLocaleString()} added
                      {h.duplicates > 0 && `, ${h.duplicates.toLocaleString()} duplicates`}
                    </td>
                    <td className="right">
                      <button
                        className="btn subtle"
                        onClick={() => {
                          if (confirm("Remove this import and its plays?")) {
                            void deleteImport(h.id).then(reload);
                          }
                        }}
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}
    </div>
  );
}
