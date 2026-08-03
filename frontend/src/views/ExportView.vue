<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useJournalStore } from "../stores/journal";
import { useAppStore } from "../stores/app";

interface ExportTemplate {
  id: string;
  name: string;
  description: string | null;
  format: string;
  body: string;
  is_builtin: boolean;
}

interface MultiPreview {
  title: string;
  format: string;
  content: string;
  note: string | null;
}

const journal = useJournalStore();
const app = useAppStore();

const mode = ref<"entry" | "period">("entry");
const formats = ["markdown", "typst", "latex", "html", "docx"];
const format = ref("typst");
const templates = ref<ExportTemplate[]>([]);
const templateId = ref<string | null>(null);
const entryId = ref<string | null>(null);
const fromDate = ref(new Date(Date.now() - 30 * 864e5).toISOString().slice(0, 10));
const toDate = ref(new Date().toISOString().slice(0, 10));
const author = ref("");
const project = ref("");
const reportTitle = ref("Laboratory report");
const tagFilter = ref("");
const preview = ref<MultiPreview | null>(null);
const loading = ref(false);

const filteredTemplates = computed(() =>
  templates.value.filter((t) => t.format === format.value || format.value === "docx"),
);

onMounted(async () => {
  await journal.refresh();
  try {
    templates.value = await invoke<ExportTemplate[]>("list_export_templates");
    const first = templates.value.find((t) => t.format === format.value);
    templateId.value = first?.id ?? null;
  } catch (e) {
    app.setError(String(e));
  }
  if (journal.entries[0]) entryId.value = journal.entries[0].id;
});

function onFormatChange() {
  const first = templates.value.find((t) => t.format === format.value);
  templateId.value = first?.id ?? null;
  preview.value = null;
}

async function doPreview() {
  loading.value = true;
  app.setError(null);
  try {
    if (mode.value === "entry") {
      if (!entryId.value) throw new Error("Выберите запись журнала");
      preview.value = await invoke<MultiPreview>("preview_entry_export", {
        entryId: entryId.value,
        format: format.value,
        templateId: templateId.value,
        author: author.value || null,
        project: project.value || null,
      });
    } else {
      preview.value = await invoke<MultiPreview>("preview_period_export", {
        fromDate: fromDate.value,
        toDate: toDate.value,
        format: format.value,
        templateId: templateId.value,
        title: reportTitle.value || null,
        author: author.value || null,
        project: project.value || null,
        tagFilter: tagFilter.value || null,
      });
    }
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function doExport() {
  if (!preview.value) await doPreview();
  if (!preview.value) return;
  const ext =
    format.value === "markdown"
      ? "md"
      : format.value === "typst"
        ? "typ"
        : format.value === "latex"
          ? "tex"
          : format.value === "html"
            ? "html"
            : "docx";
  const path = await save({
    defaultPath: `${preview.value.title || "export"}.${ext}`,
    filters: [{ name: format.value, extensions: [ext] }],
  });
  if (!path) return;
  loading.value = true;
  try {
    if (mode.value === "entry") {
      await invoke("export_entry_formatted", {
        entryId: entryId.value,
        format: format.value,
        path,
        templateId: templateId.value,
        author: author.value || null,
        project: project.value || null,
      });
    } else {
      await invoke("export_period_formatted", {
        fromDate: fromDate.value,
        toDate: toDate.value,
        format: format.value,
        path,
        templateId: templateId.value,
        title: reportTitle.value || null,
        author: author.value || null,
        project: project.value || null,
        tagFilter: tagFilter.value || null,
      });
    }
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Экспорт</h1>
      <div class="toolbar">
        <button class="btn" :class="{ 'btn-primary': mode === 'entry' }" @click="mode = 'entry'">
          Одна запись
        </button>
        <button class="btn" :class="{ 'btn-primary': mode === 'period' }" @click="mode = 'period'">
          Отчёт за период
        </button>
      </div>
    </header>
    <div class="page-body export-layout">
      <div class="card panel">
        <label class="field">
          <span class="muted">Формат</span>
          <select v-model="format" class="input" @change="onFormatChange">
            <option v-for="f in formats" :key="f" :value="f">{{ f }}</option>
          </select>
        </label>
        <label class="field">
          <span class="muted">Export-template</span>
          <select v-model="templateId" class="input">
            <option :value="null">— default —</option>
            <option v-for="t in filteredTemplates" :key="t.id" :value="t.id">
              {{ t.name }}
            </option>
          </select>
        </label>
        <label class="field">
          <span class="muted">Автор</span>
          <input v-model="author" class="input" />
        </label>
        <label class="field">
          <span class="muted">Проект</span>
          <input v-model="project" class="input" />
        </label>

        <template v-if="mode === 'entry'">
          <label class="field">
            <span class="muted">Запись журнала</span>
            <select v-model="entryId" class="input">
              <option v-for="e in journal.entries" :key="e.id" :value="e.id">
                {{ e.entry_date }} — {{ e.title }}
              </option>
            </select>
          </label>
        </template>
        <template v-else>
          <label class="field">
            <span class="muted">Заголовок отчёта</span>
            <input v-model="reportTitle" class="input" />
          </label>
          <div class="row">
            <label class="field">
              <span class="muted">С</span>
              <input v-model="fromDate" type="date" class="input" />
            </label>
            <label class="field">
              <span class="muted">По</span>
              <input v-model="toDate" type="date" class="input" />
            </label>
          </div>
          <label class="field">
            <span class="muted">Фильтр тега (опц.)</span>
            <input v-model="tagFilter" class="input" placeholder="синтез" />
          </label>
        </template>

        <div class="toolbar" style="margin-top: 12px">
          <button class="btn btn-primary" :disabled="loading" @click="doPreview">
            {{ loading ? "…" : "Предпросмотр" }}
          </button>
          <button class="btn" :disabled="loading || !preview" @click="doExport">
            Сохранить файл…
          </button>
        </div>
      </div>

      <div class="card preview">
        <div v-if="!preview" class="muted">Выберите параметры и нажмите «Предпросмотр».</div>
        <template v-else>
          <div class="toolbar" style="margin-bottom: 8px">
            <strong>{{ preview.title }}</strong>
            <span class="badge">{{ preview.format }}</span>
            <span v-if="preview.note" class="muted" style="font-size: 0.85rem">{{ preview.note }}</span>
          </div>
          <pre class="mono">{{ preview.content }}</pre>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.export-layout {
  display: grid;
  grid-template-columns: 320px 1fr;
  gap: 16px;
  max-width: 1200px;
}
@media (max-width: 900px) {
  .export-layout {
    grid-template-columns: 1fr;
  }
}
.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: fit-content;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.input {
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
}
.preview {
  min-height: 360px;
  overflow: auto;
}
.mono {
  font-family: var(--mono);
  font-size: 0.82rem;
  white-space: pre-wrap;
  margin: 0;
}
</style>
