import { useState } from "react";
import { importFiles, pickFiles, previewSoundcloud } from "../../api";
import type { ImportReport, SoundcloudMapping, SoundcloudPreview } from "../../api/types";
import { formatClock } from "../dashboard/format";
import { ReportLine } from "./ReportLine";
import { SourceShell } from "./SourceCard";

const FIELDS: { key: keyof SoundcloudMapping; label: string; required?: boolean }[] = [
  { key: "time", label: "Played at", required: true },
  { key: "title", label: "Track title" },
  { key: "artist", label: "Artist" },
  { key: "duration", label: "Duration" },
  { key: "url", label: "Track URL" },
];

/** SoundCloud's export format is undocumented, so show what was detected first. */
export function SoundcloudImport() {
  const [paths, setPaths] = useState<string[] | null>(null);
  const [preview, setPreview] = useState<SoundcloudPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const guard = async (f: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const choose = () =>
    guard(async () => {
      const picked = await pickFiles(["zip", "json", "csv"]);
      if (!picked) return;
      setReport(null);
      setPaths(picked);
      setPreview(await previewSoundcloud(picked));
    });

  const remap = (key: keyof SoundcloudMapping, column: string) =>
    guard(async () => {
      if (!paths || !preview) return;
      const mapping = { ...preview.mapping, [key]: column || null };
      setPreview(await previewSoundcloud(paths, mapping));
    });

  const confirm = () =>
    guard(async () => {
      if (!paths || !preview) return;
      setReport(await importFiles("soundcloud", paths, preview.mapping));
      setPreview(null);
      setPaths(null);
    });

  return (
    <SourceShell
      title="SoundCloud"
      color="var(--soundcloud)"
      steps={[
        "Request a copy of your personal data from SoundCloud (Settings → Privacy, or a data access request via their help center).",
        "Choose the downloaded archive or the listening history file.",
        "Check the detected columns, then import.",
      ]}
      note="SoundCloud includes only your last 1,000 plays. Missing durations are estimated."
      wide={preview !== null}
    >
      {!preview && (
        <button className="btn primary" disabled={busy} onClick={() => void choose()}>
          {busy ? "Reading…" : "Choose files…"}
        </button>
      )}

      {preview && (
        <div className="stack mapper">
          <div className="muted small">{preview.rows.toLocaleString()} rows found</div>
          <div className="mapping">
            {FIELDS.map((f) => (
              <label key={f.key}>
                <span className="small">
                  {f.label}
                  {f.required && " *"}
                </span>
                <select
                  value={preview.mapping[f.key] ?? ""}
                  disabled={busy}
                  onChange={(e) => void remap(f.key, e.target.value)}
                >
                  <option value="">—</option>
                  {preview.columns.map((c) => (
                    <option key={c} value={c}>
                      {c}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>
          <table className="list small">
            <tbody>
              {preview.sample.map((r, i) => (
                <tr key={i}>
                  <td>{r.time ? new Date(r.time).toLocaleString() : <em>no time</em>}</td>
                  <td>{r.artist ?? <em className="muted">no artist</em>}</td>
                  <td>{r.title ?? <em className="muted">no title</em>}</td>
                  <td className="right">{r.duration_ms ? formatClock(r.duration_ms) : "~"}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="row">
            <button
              className="btn primary"
              disabled={busy || !preview.mapping.time}
              onClick={() => void confirm()}
            >
              {busy ? "Importing…" : `Import ${preview.rows.toLocaleString()} rows`}
            </button>
            <button className="btn subtle" disabled={busy} onClick={() => setPreview(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}
      {report && <ReportLine report={report} />}
      {error && <p className="error small">{error}</p>}
    </SourceShell>
  );
}
