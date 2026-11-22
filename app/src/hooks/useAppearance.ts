// Theme and density preferences: persisted per viewer and applied to the document root.

import { useEffect, useState } from "react";
import { THEMES, readStoredChoice } from "../themes";
import type { DensityId, ThemeId } from "../types";
import { storeItem } from "./storage";

export function useAppearance() {
  const [theme, setTheme] = useState<ThemeId>(() =>
    readStoredChoice(
      "aj-theme",
      THEMES.map((t) => t.id),
      "github-light",
    ),
  );
  const [density, setDensity] = useState<DensityId>(() =>
    readStoredChoice("aj-density", ["compact", "default", "comfortable"], "default"),
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
