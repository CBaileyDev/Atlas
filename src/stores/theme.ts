/**
 * Theme store — light | dark. Persisted to localStorage, applied to
 * <html data-theme="..."> so the CSS variable swap is instant.
 *
 * Backend persistence (settings.json) lands once we have multiple
 * preferences to round-trip; for now localStorage is the source of
 * truth.
 */

import { create } from "zustand";

export type Theme = "dark" | "light";

const STORAGE_KEY = "atlas:theme";

function readInitial(): Theme {
  if (typeof window === "undefined") return "dark";
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === "light" || stored === "dark") return stored;
  // Honor the OS preference on first launch.
  if (window.matchMedia?.("(prefers-color-scheme: light)").matches) {
    return "light";
  }
  return "dark";
}

function apply(theme: Theme) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = theme;
}

interface ThemeStore {
  theme: Theme;
  setTheme: (t: Theme) => void;
  toggle: () => void;
}

export const useTheme = create<ThemeStore>((set, get) => {
  const initial = readInitial();
  apply(initial);
  return {
    theme: initial,
    setTheme(t) {
      apply(t);
      window.localStorage.setItem(STORAGE_KEY, t);
      set({ theme: t });
    },
    toggle() {
      const next: Theme = get().theme === "dark" ? "light" : "dark";
      get().setTheme(next);
    },
  };
});
