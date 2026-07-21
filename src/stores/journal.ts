import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { ExportPreview, JournalEntry, TemplateRecord } from "../types";
import { useAppStore } from "./app";

export const useJournalStore = defineStore("journal", {
  state: () => ({
    entries: [] as JournalEntry[],
    templates: [] as TemplateRecord[],
    current: null as JournalEntry | null,
    preview: null as ExportPreview | null,
    loading: false,
  }),
  actions: {
    async refresh() {
      const app = useAppStore();
      try {
        const [entries, templates] = await Promise.all([
          invoke<JournalEntry[]>("list_journal_entries"),
          invoke<TemplateRecord[]>("list_templates"),
        ]);
        this.entries = entries;
        this.templates = templates;
      } catch (e) {
        app.setError(String(e));
      }
    },

    async loadEntry(id: string) {
      const app = useAppStore();
      try {
        this.current = await invoke<JournalEntry>("get_journal_entry", { id });
      } catch (e) {
        app.setError(String(e));
      }
    },

    async create(input: {
      title: string;
      template_id?: string | null;
      body_md: string;
      fields?: Record<string, unknown>;
      tags?: string[];
      entry_date?: string;
    }) {
      const app = useAppStore();
      this.loading = true;
      try {
        const entry = await invoke<JournalEntry>("create_journal_entry", {
          input: {
            title: input.title,
            template_id: input.template_id ?? null,
            body_md: input.body_md,
            fields: input.fields ?? {},
            tags: input.tags ?? [],
            entry_date: input.entry_date ?? null,
          },
        });
        this.current = entry;
        await this.refresh();
        return entry;
      } catch (e) {
        app.setError(String(e));
        return null;
      } finally {
        this.loading = false;
      }
    },

    async update(
      id: string,
      input: {
        title: string;
        template_id?: string | null;
        body_md: string;
        fields?: Record<string, unknown>;
        tags?: string[];
        entry_date?: string;
      },
    ) {
      const app = useAppStore();
      this.loading = true;
      try {
        const entry = await invoke<JournalEntry>("update_journal_entry", {
          id,
          input: {
            title: input.title,
            template_id: input.template_id ?? null,
            body_md: input.body_md,
            fields: input.fields ?? {},
            tags: input.tags ?? [],
            entry_date: input.entry_date ?? null,
          },
        });
        this.current = entry;
        await this.refresh();
        return entry;
      } catch (e) {
        app.setError(String(e));
        return null;
      } finally {
        this.loading = false;
      }
    },

    async remove(id: string) {
      const app = useAppStore();
      try {
        await invoke("delete_journal_entry", { id });
        if (this.current?.id === id) this.current = null;
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      }
    },

    async previewExport(id: string) {
      const app = useAppStore();
      try {
        this.preview = await invoke<ExportPreview>("preview_journal_export", { id });
      } catch (e) {
        app.setError(String(e));
      }
    },

    async exportToFile(id: string) {
      const app = useAppStore();
      try {
        await this.previewExport(id);
        const path = await save({
          filters: [{ name: "Markdown", extensions: ["md"] }],
          defaultPath: `${this.preview?.title || "entry"}.md`,
        });
        if (!path) return;
        await invoke("export_journal_entry", { id, path });
      } catch (e) {
        app.setError(String(e));
      }
    },

    async createTemplate(input: {
      name: string;
      description?: string;
      category?: string;
      fields: unknown[];
      body_md: string;
      default_tags?: string[];
    }) {
      const app = useAppStore();
      try {
        await invoke("create_template", { input });
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      }
    },

    async saveEntryAsTemplate(entryId: string, name: string) {
      const app = useAppStore();
      try {
        await invoke("save_entry_as_template", { entryId, name });
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      }
    },

    async openInChroma(spectrumPath: string) {
      const app = useAppStore();
      try {
        await invoke("open_in_chroma", {
          spectrumPath,
          range: null,
        });
      } catch (e) {
        app.setError(String(e));
      }
    },

    async exportTemplateFile(id: string) {
      const app = useAppStore();
      try {
        const path = await save({
          filters: [{ name: "SoheiDesk template", extensions: ["json"] }],
          defaultPath: "template.json",
        });
        if (!path) return;
        await invoke("export_template_file", { id, path });
      } catch (e) {
        app.setError(String(e));
      }
    },

    async importTemplateFile() {
      const app = useAppStore();
      try {
        const path = await open({
          multiple: false,
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (typeof path !== "string") return;
        await invoke("import_template_file", { path });
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      }
    },

    clearPreview() {
      this.preview = null;
    },
  },
});
