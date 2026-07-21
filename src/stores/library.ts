import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { DocumentRecord, OpenResult } from "../types";
import { useAppStore } from "./app";

export const useLibraryStore = defineStore("library", {
  state: () => ({
    documents: [] as DocumentRecord[],
    current: null as OpenResult | null,
    loading: false,
  }),
  actions: {
    async refresh() {
      const app = useAppStore();
      try {
        this.documents = await invoke<DocumentRecord[]>("list_documents");
      } catch (e) {
        app.setError(String(e));
      }
    },

    async openViaDialog() {
      const app = useAppStore();
      this.loading = true;
      app.setError(null);
      try {
        const selected = await open({
          multiple: false,
          filters: [
            {
              name: "Documents",
              extensions: [
                "pdf",
                "md",
                "markdown",
                "txt",
                "text",
                "log",
                "docx",
                "epub",
                "html",
                "htm",
                "tex",
                "latex",
                "ltx",
              ],
            },
          ],
        });
        if (!selected || Array.isArray(selected)) {
          return;
        }
        const result = await invoke<OpenResult>("open_document_path", {
          path: selected,
        });
        this.current = result;
        // index for Tantivy (best-effort)
        try {
          await invoke("index_document", {
            id: result.document.id,
            title: result.opened.title,
            path: result.opened.path,
            docType: result.opened.doc_type,
            text: result.opened.text,
          });
        } catch {
          /* non-fatal */
        }
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      } finally {
        this.loading = false;
      }
    },

    async openById(id: string) {
      const app = useAppStore();
      this.loading = true;
      app.setError(null);
      try {
        const result = await invoke<OpenResult>("reopen_document", { id });
        this.current = result;
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      } finally {
        this.loading = false;
      }
    },

    async remove(id: string) {
      const app = useAppStore();
      try {
        await invoke("remove_document", { id });
        if (this.current?.document.id === id) {
          this.current = null;
        }
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      }
    },

    clearCurrent() {
      this.current = null;
    },
  },
});
