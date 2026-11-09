import { create } from "zustand";
import type { MirrorOptions } from "@/types";
import { readJson, writeJson } from "@/utils/storage";

export type Theme = "dark" | "light";

const THEME_KEY = "openvysor.theme";
const OPTIONS_KEY = "openvysor.mirrorOptions";

export const DEFAULT_OPTIONS: MirrorOptions = { maxSize: 1600, bitRate: 8_000_000, maxFps: 60 };

interface SettingsStore {
  theme: Theme;
  options: MirrorOptions;
  setTheme: (theme: Theme) => void;
  setOptions: (options: MirrorOptions) => void;
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  theme: readJson<Theme>(THEME_KEY, "dark"),
  options: { ...DEFAULT_OPTIONS, ...readJson<Partial<MirrorOptions>>(OPTIONS_KEY, {}) },
  setTheme: (theme) => {
    writeJson(THEME_KEY, theme);
    applyTheme(theme);
    set({ theme });
  },
  setOptions: (options) => {
    writeJson(OPTIONS_KEY, options);
    set({ options });
  },
}));
