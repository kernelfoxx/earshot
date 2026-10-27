import { useState, type ReactNode } from "react";
import { importFiles, pickFiles } from "../../api";
import type { ImportKind, ImportReport } from "../../api/types";
import { ReportLine } from "./ReportLine";

interface Props {
  kind: ImportKind;
  title: string;
  color: string;
  extensions: string[];
  steps: string[];
  note?: string;
}

export function SourceCard({ kind, title, color, extensions, steps, note }: Props) {
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    const paths = await pickFiles(extensions);
    if (!paths) return;
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      setReport(await importFiles(kind, paths));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <SourceShell title={title} color={color} steps={steps} note={note}>
      <button className="btn primary" disabled={busy} onClick={() => void run()}>
        {busy ? "Importing…" : "Choose files…"}
      </button>
      {report && <ReportLine report={report} />}
      {error && <p className="error small">{error}</p>}
    </SourceShell>
  );
}

export function SourceShell(props: {
  title: string;
  color: string;
  steps: string[];
  note?: string;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={props.wide ? "card source-card wide" : "card source-card"}>
      <h3>
        <span className="dot" style={{ background: props.color }} />
        {props.title}
      </h3>
      <ol className="steps">
        {props.steps.map((s) => (
          <li key={s}>{s}</li>
        ))}
      </ol>
      {props.note && <p className="muted small">{props.note}</p>}
      <div className="source-actions">{props.children}</div>
    </div>
  );
}
