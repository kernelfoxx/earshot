import { useState } from "react";
import { Sidebar, type Page } from "./components/Sidebar";
import { Dashboard } from "./features/dashboard/Dashboard";
import { Placeholder } from "./components/Placeholder";

const titles: Record<Page, string> = {
  overview: "Overview",
  stats: "Statistics",
  recap: "Recap",
  import: "Import",
  settings: "Settings",
};

export function App() {
  const [page, setPage] = useState<Page>("overview");

  return (
    <div className="app">
      <Sidebar current={page} onNavigate={setPage} />
      <main className="content">
        <h1 className="page-title">{titles[page]}</h1>
        {page === "overview" ? <Dashboard /> : <Placeholder />}
      </main>
    </div>
  );
}
