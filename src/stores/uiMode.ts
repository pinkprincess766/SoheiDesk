import { defineStore } from "pinia";

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
    setMode(mode: UiMode) {
      this.mode = mode;
      localStorage.setItem(KEY, mode);
    },
    /** Clear choice → picker on next full reload */
    resetMode() {
      this.mode = null;
      localStorage.removeItem(KEY);
    },
  },
});
