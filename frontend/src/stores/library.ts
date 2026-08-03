import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { DocumentRecord, OpenResult } from "../types";
import { useAppStore } from "./app";
import { useUiModeStore } from "./uiMode";

const OPEN_EXTENSIONS = [
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
  "fb2",
  "djvu",
  "djv",
  "rtf",
];

export const useLibraryStore = defineStore("library", {
  state: () => ({
    documents: [] as DocumentRecord[],
    current: null as OpenResult | null,
    loading: false,
    /** Status line for Simple mode (e.g. extracting PDF text) */
    status: "" as string,
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

    /** Open an absolute filesystem path and load into the reader. */
    async openPath(path: string) {
      const app = useAppStore();
      this.loading = true;
      this.status = "Открытие…";
      app.setError(null);
      try {
        // Backend extracts PDF text + caches body.txt (Simple + Normal)
        this.status = "Чтение файла / извлечение текста…";
        const result = await invoke<OpenResult>("open_document_path", { path });
        this.current = result;
        this.status = "";

        // Index only in normal mode
        if (useUiModeStore().isNormal) {
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
        }
        await this.refresh();
        return result;
      } catch (e) {
        app.setError(String(e));
        this.status = "";
        return null;
      } finally {
        this.loading = false;
        this.status = "";
      }
    },

    async openViaDialog() {
      try {
        const selected = await open({
          multiple: false,
          filters: [{ name: "Documents", extensions: OPEN_EXTENSIONS }],
        });
        if (!selected || Array.isArray(selected)) return null;
        return await this.openPath(selected);
      } catch (e) {
        useAppStore().setError(String(e));
        return null;
      }
    },

    async openById(id: string) {
      const app = useAppStore();
      this.loading = true;
      this.status = "Открытие…";
      app.setError(null);
      try {
        const result = await invoke<OpenResult>("reopen_document", { id });
        this.current = result;
        await this.refresh();
        return result;
      } catch (e) {
        app.setError(String(e));
        return null;
      } finally {
        this.loading = false;
        this.status = "";
      }
    },

    async remove(id: string) {
      const app = useAppStore();
      try {
        await invoke("remove_document", { id });
        if (this.current?.document.id === id) this.current = null;
        await this.refresh();
      } catch (e) {
        app.setError(String(e));
      }
    },

    clearCurrent() {
      this.current = null;
      this.status = "";
    },
  },
});
