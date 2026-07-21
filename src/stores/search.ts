import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "./app";

export interface SearchHit {
  id: string;
  kind: string;
  title: string;
  snippet: string;
  path: string | null;
  score: number;
}

export const useSearchStore = defineStore("search", {
  state: () => ({
    query: "",
    hits: [] as SearchHit[],
    loading: false,
  }),
  actions: {
    async run(query?: string) {
      const app = useAppStore();
      if (query !== undefined) this.query = query;
      if (!this.query.trim()) {
        this.hits = [];
        return;
      }
      this.loading = true;
      try {
        this.hits = await invoke<SearchHit[]>("search_all", {
          query: this.query,
          limit: 40,
        });
      } catch (e) {
        app.setError(String(e));
      } finally {
        this.loading = false;
      }
    },
    async reindex() {
      const app = useAppStore();
      try {
        const n = await invoke<number>("reindex_all");
        app.setError(null);
        return n;
      } catch (e) {
        app.setError(String(e));
        return 0;
      }
    },
  },
});
