<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useJournalStore } from "../stores/journal";
import type { TemplateField, TemplateRecord } from "../types";
import { renderMarkdown } from "../utils/markdown";

const journal = useJournalStore();

const screen = ref<"list" | "edit" | "templates" | "preview">("list");
const title = ref("");
const bodyMd = ref("");
const entryDate = ref(new Date().toISOString().slice(0, 10));
const tagsStr = ref("");
const fieldValues = ref<Record<string, string>>({});
const selectedTemplateId = ref<string | null>(null);
const editingId = ref<string | null>(null);

// template editor
const tplName = ref("");
const tplBody = ref("");
const tplFieldsJson = ref("[]");

const selectedTemplate = computed(() =>
  journal.templates.find((t) => t.id === selectedTemplateId.value) || null,
);

const fields = computed<TemplateField[]>(() => {
  if (!selectedTemplate.value) return [];
  try {
    return JSON.parse(selectedTemplate.value.fields_json);
  } catch {
    return [];
  }
});

onMounted(() => journal.refresh());

function parseTags() {
  return tagsStr.value
    .split(/[,#]+/)
    .map((t) => t.trim())
    .filter(Boolean);
}

function startNew(tpl?: TemplateRecord) {
  editingId.value = null;
  selectedTemplateId.value = tpl?.id ?? null;
  title.value = "";
  bodyMd.value = tpl?.body_md ?? "";
  entryDate.value = new Date().toISOString().slice(0, 10);
  tagsStr.value = "";
  fieldValues.value = {};
  if (tpl) {
    try {
      const f: TemplateField[] = JSON.parse(tpl.fields_json);
      for (const field of f) {
        fieldValues.value[field.key] = "";
      }
    } catch {
      /* */
    }
  }
  screen.value = "edit";
}

function openEntry(id: string) {
  const e = journal.entries.find((x) => x.id === id);
  if (!e) return;
  editingId.value = e.id;
  selectedTemplateId.value = e.template_id;
  title.value = e.title;
  bodyMd.value = e.body_md;
  entryDate.value = e.entry_date;
  try {
    tagsStr.value = (JSON.parse(e.tags_json || "[]") as string[]).join(", ");
  } catch {
    tagsStr.value = "";
  }
  try {
    const f = JSON.parse(e.fields_json || "{}") as Record<string, unknown>;
    fieldValues.value = Object.fromEntries(
      Object.entries(f).map(([k, v]) => [k, v == null ? "" : String(v)]),
    );
  } catch {
    fieldValues.value = {};
  }
  screen.value = "edit";
}

async function saveEntry() {
  const payload = {
    title: title.value,
    template_id: selectedTemplateId.value,
    body_md: bodyMd.value,
    fields: { ...fieldValues.value },
    tags: parseTags(),
    entry_date: entryDate.value,
  };
  if (editingId.value) {
    await journal.update(editingId.value, payload);
  } else {
    const e = await journal.create(payload);
    if (e) editingId.value = e.id;
  }
}

async function pickFile(key: string) {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Data", extensions: ["csv", "txt", "dat", "png", "jpg", "jpeg", "pdf"] }],
  });
  if (typeof selected === "string") {
    fieldValues.value[key] = selected;
  }
}

async function doPreview() {
  if (!editingId.value) {
    await saveEntry();
  }
  if (!editingId.value) return;
  await journal.previewExport(editingId.value);
  screen.value = "preview";
}

async function saveAsTemplate() {
  if (!editingId.value) {
    await saveEntry();
  }
  if (!editingId.value) return;
  const name = window.prompt("Имя шаблона:", title.value || "Мой шаблон");
  if (!name) return;
  await journal.saveEntryAsTemplate(editingId.value, name);
}

async function createCustomTemplate() {
  let fields: unknown[] = [];
  try {
    fields = JSON.parse(tplFieldsJson.value);
  } catch {
    alert("fields JSON invalid");
    return;
  }
  await journal.createTemplate({
    name: tplName.value,
    body_md: tplBody.value,
    fields,
    category: "custom",
  });
  tplName.value = "";
  tplBody.value = "";
  tplFieldsJson.value = "[]";
  screen.value = "list";
}

const previewHtml = computed(() =>
  journal.preview ? renderMarkdown(journal.preview.markdown) : "",
);
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Лабораторный журнал</h1>
      <div class="toolbar">
        <button v-if="screen !== 'list'" class="btn" @click="screen = 'list'; journal.clearPreview()">
          ← К списку
        </button>
        <button class="btn" @click="screen = 'templates'">Шаблоны</button>
        <button class="btn btn-primary" @click="startNew()">Новая запись</button>
      </div>
    </header>

    <!-- LIST -->
    <div v-if="screen === 'list'" class="page-body">
      <div class="card" style="margin-bottom: 16px">
        <div class="muted" style="margin-bottom: 8px">Создать из шаблона</div>
        <div class="toolbar" style="flex-wrap: wrap">
          <button
            v-for="t in journal.templates"
            :key="t.id"
            class="btn"
            @click="startNew(t)"
          >
            {{ t.name }}
            <span v-if="t.is_builtin" class="badge" style="margin-left: 6px">built-in</span>
          </button>
        </div>
      </div>

      <div v-if="journal.entries.length === 0" class="empty" style="min-height: 200px">
        <p class="muted">Записей пока нет — выберите шаблон выше.</p>
      </div>
      <div v-else class="list">
        <div v-for="e in journal.entries" :key="e.id" class="list-item">
          <div>
            <strong>{{ e.title }}</strong>
            <div class="muted" style="font-size: 0.85rem">{{ e.entry_date }}</div>
          </div>
          <div class="toolbar">
            <button class="btn btn-primary" @click="openEntry(e.id)">Открыть</button>
            <button class="btn" @click="journal.previewExport(e.id).then(() => (screen = 'preview'))">
              Preview
            </button>
            <button class="btn btn-danger" @click="journal.remove(e.id)">Удалить</button>
          </div>
        </div>
      </div>
    </div>

    <!-- EDIT -->
    <div v-else-if="screen === 'edit'" class="page-body" style="max-width: 800px">
      <div class="card" style="display: flex; flex-direction: column; gap: 12px">
        <label>
          <div class="muted" style="font-size: 0.8rem">Заголовок *</div>
          <input v-model="title" class="input" placeholder="Название записи" />
        </label>
        <label>
          <div class="muted" style="font-size: 0.8rem">Дата</div>
          <input v-model="entryDate" type="date" class="input" />
        </label>
        <label>
          <div class="muted" style="font-size: 0.8rem">Теги (через запятую)</div>
          <input v-model="tagsStr" class="input" placeholder="синтез, ик" />
        </label>

        <div v-if="selectedTemplate" class="muted" style="font-size: 0.85rem">
          Шаблон: <strong>{{ selectedTemplate.name }}</strong>
        </div>

        <div v-for="f in fields" :key="f.key" style="display: flex; flex-direction: column; gap: 4px">
          <div class="muted" style="font-size: 0.8rem">
            {{ f.label }} <span v-if="f.required">*</span>
          </div>
          <textarea
            v-if="f.type === 'textarea'"
            v-model="fieldValues[f.key]"
            class="input"
            rows="3"
          />
          <div v-else-if="f.type === 'file'" class="toolbar">
            <input v-model="fieldValues[f.key]" class="input" style="flex: 1" readonly placeholder="path…" />
            <button class="btn" @click="pickFile(f.key)">…</button>
            <button
              v-if="fieldValues[f.key]"
              class="btn"
              @click="journal.openInChroma(fieldValues[f.key])"
            >
              ChromaTsvet
            </button>
          </div>
          <input v-else v-model="fieldValues[f.key]" class="input" :type="f.type === 'number' ? 'number' : f.type === 'date' ? 'date' : 'text'" />
        </div>

        <label>
          <div class="muted" style="font-size: 0.8rem">Тело (Markdown)</div>
          <textarea v-model="bodyMd" class="input mono" rows="12" />
        </label>

        <div class="toolbar">
          <button class="btn btn-primary" :disabled="journal.loading" @click="saveEntry">
            Сохранить
          </button>
          <button class="btn" @click="doPreview">Предпросмотр MD</button>
          <button class="btn" @click="saveAsTemplate">Сохранить как шаблон</button>
          <button
            v-if="editingId"
            class="btn"
            @click="journal.exportToFile(editingId)"
          >
            Экспорт в файл…
          </button>
        </div>
      </div>
    </div>

    <!-- PREVIEW -->
    <div v-else-if="screen === 'preview'" class="page-body">
      <div class="toolbar" style="margin-bottom: 12px">
        <button
          v-if="editingId || journal.preview"
          class="btn btn-primary"
          @click="editingId && journal.exportToFile(editingId)"
        >
          Сохранить .md…
        </button>
        <button class="btn" @click="screen = editingId ? 'edit' : 'list'">Назад</button>
      </div>
      <div class="card">
        <pre class="mono" style="white-space: pre-wrap; margin: 0 0 16px">{{ journal.preview?.markdown }}</pre>
        <hr style="border-color: var(--border)" />
        <div class="md-viewer" v-html="previewHtml" />
      </div>
    </div>

    <!-- TEMPLATES -->
    <div v-else-if="screen === 'templates'" class="page-body" style="max-width: 720px">
      <div class="toolbar" style="margin-bottom: 12px">
        <button class="btn" @click="journal.importTemplateFile()">Импорт .json…</button>
      </div>
      <div class="list" style="margin-bottom: 20px">
        <div v-for="t in journal.templates" :key="t.id" class="list-item">
          <div>
            <strong>{{ t.name }}</strong>
            <span v-if="t.is_builtin" class="badge" style="margin-left: 8px">builtin</span>
            <div class="muted" style="font-size: 0.85rem">{{ t.description }}</div>
          </div>
          <div class="toolbar">
            <button class="btn" @click="journal.exportTemplateFile(t.id)">Export</button>
            <button class="btn btn-primary" @click="startNew(t)">Использовать</button>
          </div>
        </div>
      </div>
      <div class="card" style="display: flex; flex-direction: column; gap: 10px">
        <h3 style="margin: 0">Новый шаблон</h3>
        <input v-model="tplName" class="input" placeholder="Имя" />
        <textarea v-model="tplBody" class="input mono" rows="6" placeholder="body markdown" />
        <textarea
          v-model="tplFieldsJson"
          class="input mono"
          rows="6"
          placeholder='fields JSON: [{"key":"x","label":"X","type":"text","required":true}]'
        />
        <button class="btn btn-primary" @click="createCustomTemplate">Создать шаблон</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.input {
  width: 100%;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
}
.mono {
  font-family: var(--mono);
  font-size: 0.9rem;
}
label {
  display: block;
}
</style>
