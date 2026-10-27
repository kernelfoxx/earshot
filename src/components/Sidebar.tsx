import { ThemeSwitch } from "./ThemeSwitch";

export type Page = "overview" | "stats" | "recap" | "import" | "settings";

const items: { id: Page; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "stats", label: "Statistics" },
  { id: "recap", label: "Recap" },
  { id: "import", label: "Import" },
  { id: "settings", label: "Settings" },
];

interface Props {
  current: Page;
  onNavigate: (page: Page) => void;
}

export function Sidebar({ current, onNavigate }: Props) {
  return (
    <nav className="sidebar">
      <div className="brand">
        <img src="/tunelog.svg" alt="" width={28} height={28} />
        <span>TuneLog</span>
      </div>
      <ul>
        {items.map((item) => (
          <li key={item.id}>
            <button
              className={item.id === current ? "nav-item active" : "nav-item"}
              aria-current={item.id === current ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
            >
              {item.label}
            </button>
          </li>
        ))}
      </ul>
      <div className="sidebar-footer">
        <ThemeSwitch />
      </div>
    </nav>
  );
}
