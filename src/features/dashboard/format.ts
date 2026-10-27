/** Formats a duration as "2h 05m", "12m" or "0m". */
export function formatDuration(ms: number): string {
  const totalMinutes = Math.floor(ms / 60_000);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  if (h === 0) return `${m}m`;
  return `${h.toLocaleString("en-US")}h ${String(m).padStart(2, "0")}m`;
}
