/** Formats a duration as "2h 05m", "12m", "<1m" or "0m". */
export function formatDuration(ms: number): string {
  if (ms > 0 && ms < 60_000) return "<1m";
  const totalMinutes = Math.floor(ms / 60_000);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  if (h === 0) return `${m}m`;
  return `${h.toLocaleString("en-US")}h ${String(m).padStart(2, "0")}m`;
}

/** Formats a duration as "3:07" or "1:02:09". */
export function formatClock(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}
