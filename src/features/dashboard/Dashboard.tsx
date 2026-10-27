import { PERIODS, SERVICES } from "../../api/types";

/**
 * Service × period matrix. Values are wired to the tracker database in
 * milestone 1; until then every cell shows a dash.
 */
export function Dashboard() {
  return (
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
                <td key={p.id} className="muted">
                  —
                </td>
              ))}
            </tr>
          ))}
          <tr className="total">
            <td>All services</td>
            {PERIODS.map((p) => (
              <td key={p.id}>—</td>
            ))}
          </tr>
        </tbody>
      </table>
    </div>
  );
}
