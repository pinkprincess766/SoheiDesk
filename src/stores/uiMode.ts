import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";

export type UiMode = "simple" | "normal";

const KEY = "soheidesk-ui-mode";

export const useUiModeStore = defineStore("uiMode", {
  state: () => ({
    /** null = show launch picker */
    mode: (localStorage.getItem(KEY) as UiMode | null) || null,
  }),
  getters: {
    isSimple: (s) => s.mode === "simple",
    isNormal: (s) => s.mode === "normal",
    needsPicker: (s) => s.mode !== "simple" && s.mode !== "normal",
  },
  actions: {
    async hydrate() {
      try {
        const stored = await invoke<string | null>("get_setting", { key: "ui_mode" });
        if (stored === "simple" || stored === "normal") {
          this.mode = stored;
          localStorage.setItem(KEY, stored);
        } else if (this.mode) {
          await invoke("set_setting", { key: "ui_mode", value: this.mode });
        }
      } catch {
        // localStorage remains a safe fallback if the backend is unavailable.
      }
    },
    setMode(mode: UiMode) {
      this.mode = mode;
      localStorage.setItem(KEY, mode);
      void invoke("set_setting", { key: "ui_mode", value: mode }).catch(() => {});
    },
    /** Clear choice → picker on next full reload */
    resetMode() {
      this.mode = null;
      localStorage.removeItem(KEY);
      void invoke("delete_setting", { key: "ui_mode" }).catch(() => {});
    },
  },
});
