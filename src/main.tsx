import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { initTheme } from "./theme/theme";
import "./theme/global.css";

initTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
