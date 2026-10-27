import { useState } from "react";
import { getThemePref, setThemePref, type ThemePref } from "../theme/theme";

const options: { value: ThemePref; label: string }[] = [
  { value: "system", label: "Auto" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function ThemeSwitch() {
  const [pref, setPref] = useState<ThemePref>(getThemePref);

  return (
    <div className="segmented" role="radiogroup" aria-label="Theme">
      {options.map((o) => (
        <button
          key={o.value}
          role="radio"
          aria-checked={pref === o.value}
          className={pref === o.value ? "active" : undefined}
          onClick={() => {
            setThemePref(o.value);
            setPref(o.value);
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
