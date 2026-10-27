import type { ImportReport } from "../../api/types";

export function ReportLine({ report }: { report: ImportReport }) {
  const parts = [`${report.added.toLocaleString()} plays added`];
  if (report.duplicates) parts.push(`${report.duplicates.toLocaleString()} already tracked`);
  if (report.already_imported)
    parts.push(`${report.already_imported.toLocaleString()} imported before`);
  if (report.skipped) parts.push(`${report.skipped.toLocaleString()} skipped (not music)`);
  if (report.estimated) parts.push(`${report.estimated.toLocaleString()} with estimated length`);
  return <p className="success small">✓ {parts.join(" · ")}</p>;
}
