export type ThemePref = "system" | "light" | "dark";

const KEY = "tunelog.theme";

export function getThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // Storage unavailable: fall back to system theme.
  }
  return "system";
}

export function setThemePref(pref: ThemePref) {
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    // Not persisted, but still applied for this session.
  }
  applyTheme(pref);
}

function applyTheme(pref: ThemePref) {
  const root = document.documentElement;
  if (pref === "system") delete root.dataset.theme;
  else root.dataset.theme = pref;
}

export function initTheme() {
  applyTheme(getThemePref());
}
