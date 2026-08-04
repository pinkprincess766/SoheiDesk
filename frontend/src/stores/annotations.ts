import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import type { Annotation } from "../types";
import { useAppStore } from "./app";

export const useAnnotationsStore = defineStore("annotations", {
  state: () => ({
    items: [] as Annotation[],
    documentId: null as string | null,
    loading: false,
    activeColor: "#f7e07c",
    mode: "highlight" as
      | "highlight"
      | "comment"
      | "drawing"
      | "rect"
      | "ellipse"
      | "arrow"
      | "none",
  }),
  actions: {
    async load(documentId: string) {
      const app = useAppStore();
      this.loading = true;
      this.documentId = documentId;
      try {
        this.items = await invoke<Annotation[]>("list_annotations", { documentId });
      } catch (e) {
        app.setError(String(e));
      } finally {
        this.loading = false;
      }
    },
    clear() {
      this.items = [];
      this.documentId = null;
    },
    async create(input: {
      document_id: string;
      ann_type: string;
      page?: number | null;
      position_json: string;
      content?: string | null;
      color?: string | null;
      selected_text?: string | null;
      context_before?: string | null;
      context_after?: string | null;
    }) {
      const app = useAppStore();
      try {
        const ann = await invoke<Annotation>("create_annotation", {
          input: {
            document_id: input.document_id,
            ann_type: input.ann_type,
            page: input.page ?? null,
            position_json: input.position_json,
            content: input.content ?? null,
            color: input.color ?? this.activeColor,
            selected_text: input.selected_text ?? null,
            context_before: input.context_before ?? null,
            context_after: input.context_after ?? null,
          },
        });
        this.items.push(ann);
        return ann;
      } catch (e) {
        app.setError(String(e));
        return null;
      }
    },
    async remove(id: string) {
      const app = useAppStore();
      try {
        await invoke("delete_annotation", { id });
        this.items = this.items.filter((a) => a.id !== id);
      } catch (e) {
        app.setError(String(e));
      }
    },
  },
});
