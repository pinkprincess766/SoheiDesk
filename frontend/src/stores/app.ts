import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../types";

export const useAppStore = defineStore("app", {
  state: () => ({
    info: null as AppInfo | null,
    error: null as string | null,
    theme: (localStorage.getItem("soheidesk-theme") as "system" | "light" | "dark") || "system",
  }),
  actions: {
    async loadInfo() {
      try {
        this.info = await invoke<AppInfo>("get_app_info");
        const stored = await invoke<string | null>("get_setting", { key: "ui_theme" });
        if (stored === "system" || stored === "light" || stored === "dark") {
          this.theme = stored;
          localStorage.setItem("soheidesk-theme", stored);
          applyTheme(stored);
        } else {
          await invoke("set_setting", { key: "ui_theme", value: this.theme });
        }
      } catch (e) {
        this.setError(String(e), "app.startup");
      }
    },
    setError(message: string | null, category = "frontend") {
      this.error = message;
      if (message) {
        // The backend maps this raw UI error to a finite, content-free
        // vocabulary before writing it. This call must never mask the original
        // error if diagnostics are unavailable during startup.
        void invoke("record_diagnostic_error", { category, message }).catch(() => {});
      }
    },
    setTheme(theme: "system" | "light" | "dark") {
      this.theme = theme;
      localStorage.setItem("soheidesk-theme", theme);
      applyTheme(theme);
      void invoke("set_setting", { key: "ui_theme", value: theme }).catch((e) => {
        this.setError(String(e), "settings.theme");
      });
    },
  },
});

export function applyTheme(theme: "system" | "light" | "dark") {
  const root = document.documentElement;
  if (theme === "system") {
    root.removeAttribute("data-theme");
    return;
  }
  root.setAttribute("data-theme", theme);
}
