import type { DensityId, ThemeId } from "./types";

export interface ThemeMeta {
  id: ThemeId;
  name: string;
  mode: "light" | "dark";
  swatches: [string, string, string, string];
}

export const THEMES: ThemeMeta[] = [
  {
    id: "github-light",
    name: "GitHub Light",
    mode: "light",
    swatches: ["#f6f8fa", "#ffffff", "#0969da", "#1f2328"],
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    mode: "light",
    swatches: ["#eee8d5", "#fdf6e3", "#268bd2", "#586e75"],
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    mode: "light",
    swatches: ["#eff1f5", "#fafbfc", "#1e66f5", "#4c4f69"],
  },
  {
    id: "one-dark",
    name: "One Dark Pro",
    mode: "dark",
    swatches: ["#21252b", "#282c34", "#61afef", "#abb2bf"],
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    mode: "dark",
    swatches: ["#16161e", "#1a1b26", "#7aa2f7", "#a9b1d6"],
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    mode: "dark",
    swatches: ["#181825", "#1e1e2e", "#89b4fa", "#cdd6f4"],
  },
];

/** Spacing densities, in menu order; their names are in the locale files. */
export const DENSITY_OPTIONS: DensityId[] = ["compact", "default", "comfortable"];
