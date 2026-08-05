<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useAnnotationsStore } from "../../stores/annotations";
import { useLibraryStore } from "../../stores/library";
import { useAppStore } from "../../stores/app";

const annotations = useAnnotationsStore();
const library = useLibraryStore();
const app = useAppStore();
const colors = ["#f7e07c", "#ff9aa2", "#a0e7e5", "#b4f8c8", "#cbb2fe", "#ff6b6b"];

const modes = [
  { id: "none", label: "Просмотр" },
  { id: "highlight", label: "Выделение" },
  { id: "comment", label: "Коммент" },
  { id: "drawing", label: "Перо" },
  { id: "rect", label: "Прямоуг." },
  { id: "ellipse", label: "Овал" },
  { id: "arrow", label: "Стрелка" },
] as const;

const typeLabel: Record<string, string> = {
  highlight: "Выделение",
  comment: "Коммент",
  drawing: "Перо",
  rect: "Прямоуг.",
  ellipse: "Овал",
  arrow: "Стрелка",
};

function labelFor(a: { ann_type: string; content: string | null }) {
  const body = (a.content || "").trim();
  if (body && body !== "выделение" && body !== "овал" && body !== "прямоугольник" && body !== "стрелка" && body !== "рисунок") {
    return body;
  }
  return typeLabel[a.ann_type] || a.ann_type;
}

async function exportMd() {
  const cur = library.current;
  if (!cur) {
    app.setError("Нет открытого документа");
    return;
  }
  try {
    const path = await save({
      filters: [{ name: "Markdown", extensions: ["md"] }],
      defaultPath: `${cur.opened.title}-annotations.md`,
    });
    if (!path) return;
    await invoke("export_annotations_to_file", {
      documentId: cur.document.id,
      docTitle: cur.opened.title,
      path,
    });
  } catch (e) {
    app.setError(String(e));
  }
}
</script>

<template>
  <div class="ann-panel">
    <div class="toolbar" style="flex-wrap: wrap">
      <button
        v-for="m in modes"
        :key="m.id"
        class="btn"
        style="padding: 4px 8px; font-size: 0.75rem"
        :class="{ 'btn-primary': annotations.mode === m.id }"
        @click="annotations.mode = m.id as typeof annotations.mode"
      >
        {{ m.label }}
      </button>
    </div>
    <div class="toolbar" style="margin-top: 8px">
      <button
        v-for="c in colors"
        :key="c"
        class="color-dot"
        :style="{
          background: c,
          outline: annotations.activeColor === c ? '2px solid var(--accent)' : 'none',
        }"
        @click="annotations.activeColor = c"
      />
    </div>
    <p class="muted" style="font-size: 0.72rem; margin: 8px 0 0; line-height: 1.35">
      Выделение / овал / коммент: зажмите и тяните по странице. Коммент — модальное окно (не prompt).
    </p>
    <button class="btn" style="margin-top: 10px; width: 100%" @click="exportMd">
      Export ann → MD
    </button>
    <div class="ann-list">
      <div v-if="annotations.items.length === 0" class="muted" style="font-size: 0.85rem">
        Нет аннотаций
      </div>
      <div v-for="a in annotations.items" :key="a.id" class="ann-item">
        <div style="display: flex; justify-content: space-between; gap: 8px; align-items: center">
          <span class="badge" :style="{ borderColor: a.color || undefined }">
            {{ typeLabel[a.ann_type] || a.ann_type }}
          </span>
          <button
            class="btn btn-danger"
            style="padding: 2px 8px; font-size: 0.75rem"
            @click="annotations.remove(a.id)"
          >
            ×
          </button>
        </div>
        <div v-if="a.page != null" class="muted" style="font-size: 0.75rem; margin-top: 4px">
          стр. {{ a.page }}
        </div>
        <div
          v-if="a.anchor_status === 'needs_review'"
          class="anchor-warning"
        >
          Требует проверки
        </div>
        <div v-else-if="a.anchor_status === 'rebound'" class="muted anchor-state">
          Перепривязано после обновления
        </div>
        <div style="font-size: 0.85rem; margin-top: 4px; line-height: 1.35">
          {{ labelFor(a) }}
        </div>
        <blockquote v-if="a.selected_text" class="selected-text">
          {{ a.selected_text }}
        </blockquote>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ann-panel {
  width: 260px;
  border-left: 1px solid var(--border);
  background: var(--bg-elevated);
  padding: 12px;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.ann-list {
  margin-top: 12px;
  overflow: auto;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.ann-item {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px;
  background: var(--bg);
}
.color-dot {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 1px solid var(--border);
  padding: 0;
}
.anchor-warning {
  margin-top: 6px;
  color: var(--danger);
  font-size: 0.75rem;
  font-weight: 600;
}
.anchor-state {
  margin-top: 6px;
  font-size: 0.72rem;
}
.selected-text {
  margin: 6px 0 0;
  padding-left: 8px;
  border-left: 2px solid var(--border);
  color: var(--text-muted);
  font-size: 0.75rem;
  line-height: 1.35;
  max-height: 5.4em;
  overflow: hidden;
}
</style>
