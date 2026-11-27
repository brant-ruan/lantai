// Theme and density preferences: persisted per viewer and applied to the document root.

import { useEffect, useState } from "react";
import { THEMES } from "../themes";
import type { DensityId, ThemeId } from "../types";
import { readChoice, storeItem } from "../storage";

/** The theme this viewer chose (the default until they choose one). */
export function storedTheme(): ThemeId {
  return readChoice(
    "aj-theme",
    THEMES.map((t) => t.id),
    "github-light",
  );
}

export function useAppearance() {
  const [theme, setTheme] = useState<ThemeId>(storedTheme);
  const [density, setDensity] = useState<DensityId>(() =>
    readChoice("aj-density", ["compact", "default", "comfortable"], "default"),
  );

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    storeItem("aj-theme", theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.density = density;
    storeItem("aj-density", density);
  }, [density]);

  return { theme, setTheme, density, setDensity };
}
