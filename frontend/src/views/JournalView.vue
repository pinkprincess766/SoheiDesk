<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onBeforeRouteLeave } from "vue-router";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useJournalStore } from "../stores/journal";
import type {
  JournalDraft,
  JournalDraftPayload,
  JournalEntry,
  TemplateEditorDraftPayload,
  TemplateField,
  TemplateRecord,
} from "../types";
import { renderMarkdown } from "../utils/markdown";
import {
  isRecoverableDraft,
  persistDraftsBeforeClose,
} from "../utils/draftLifecycle";

const journal = useJournalStore();

const screen = ref<"list" | "edit" | "templates" | "preview">("list");
const title = ref("");
const bodyMd = ref("");
const entryDate = ref(new Date().toISOString().slice(0, 10));
const tagsStr = ref("");
const fieldValues = ref<Record<string, string>>({});
const selectedTemplateId = ref<string | null>(null);
const editingId = ref<string | null>(null);
const baseUpdatedAt = ref<string | null>(null);
const pendingDraft = ref<JournalDraft | null>(null);
const recoverableDrafts = ref<JournalDraft[]>([]);
const autosaveStatus = ref<"idle" | "dirty" | "saving" | "saved" | "error">("idle");
const autosaveError = ref("");
const AUTOSAVE_DELAY_MS = 700;
let autosaveTimer: ReturnType<typeof setTimeout> | null = null;
let activeDraftWrite: Promise<void> | null = null;
let suppressAutosave = false;
let lastDraftSignature = "";
let unlistenClose: UnlistenFn | null = null;

// template editor
const tplName = ref("");
const tplBody = ref("");
const tplFieldsJson = ref("[]");
const pendingTemplateDraft = ref<JournalDraft<TemplateEditorDraftPayload> | null>(null);
const templateAutosaveStatus = ref<"idle" | "dirty" | "saving" | "saved" | "error">("idle");
let templateAutosaveTimer: ReturnType<typeof setTimeout> | null = null;
let activeTemplateWrite: Promise<void> | null = null;
let lastTemplateSignature = "";
const TEMPLATE_DRAFT_KEY = "template:new";

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

const autosaveLabel = computed(() => {
  switch (autosaveStatus.value) {
    case "dirty":
      return "Есть несохранённые изменения";
    case "saving":
      return "Сохранение черновика…";
    case "saved":
      return "Все изменения сохранены";
    case "error":
      return `Ошибка автосохранения: ${autosaveError.value}`;
    default:
      return "";
  }
});

const templateAutosaveLabel = computed(() => {
  switch (templateAutosaveStatus.value) {
    case "dirty":
      return "Есть несохранённые изменения";
    case "saving":
      return "Сохранение черновика…";
    case "saved":
      return "Все изменения сохранены";
    case "error":
      return "Ошибка автосохранения";
    default:
      return "";
  }
});

function draftKey() {
  return editingId.value ? `entry:${editingId.value}` : "new";
}

function draftPayload(): JournalDraftPayload {
  return {
    title: title.value,
    template_id: selectedTemplateId.value,
    body_md: bodyMd.value,
    fields: { ...fieldValues.value },
    tags: tagsStr.value,
    entry_date: entryDate.value,
  };
}

function payloadSignature(payload = draftPayload()) {
  return JSON.stringify(payload);
}

function hasDraftContent(payload: JournalDraftPayload) {
  return Boolean(
    payload.title.trim() ||
      payload.body_md.trim() ||
      payload.tags.trim() ||
      Object.values(payload.fields).some((v) => String(v).trim()),
  );
}

function templatePayload(): TemplateEditorDraftPayload {
  return {
    name: tplName.value,
    body_md: tplBody.value,
    fields_json: tplFieldsJson.value,
  };
}

function templateSignature(payload = templatePayload()) {
  return JSON.stringify(payload);
}

function hasTemplateContent(payload: TemplateEditorDraftPayload) {
  return Boolean(
    payload.name.trim() ||
      payload.body_md.trim() ||
      (payload.fields_json.trim() && payload.fields_json.trim() !== "[]"),
  );
}

async function persistTemplateDraft(): Promise<boolean> {
  if (templateAutosaveTimer) {
    clearTimeout(templateAutosaveTimer);
    templateAutosaveTimer = null;
  }
  if (screen.value !== "templates" || pendingTemplateDraft.value) return true;
  if (activeTemplateWrite) await activeTemplateWrite;
  const payload = templatePayload();
  const signature = templateSignature(payload);
  if (signature === lastTemplateSignature) return true;
  if (!hasTemplateContent(payload)) {
    lastTemplateSignature = signature;
    templateAutosaveStatus.value = "idle";
    return true;
  }

  templateAutosaveStatus.value = "saving";
  const write = (async () => {
    try {
      await journal.saveDraft<TemplateEditorDraftPayload>({
        draft_key: TEMPLATE_DRAFT_KEY,
        entry_id: null,
        payload,
        base_updated_at: null,
      });
      lastTemplateSignature = signature;
      templateAutosaveStatus.value =
        templateSignature() === signature ? "saved" : "dirty";
      if (templateAutosaveStatus.value === "dirty") scheduleTemplateAutosave();
    } catch (e) {
      templateAutosaveStatus.value = "error";
      throw e;
    }
  })();
  activeTemplateWrite = write;
  try {
    await write;
    return true;
  } catch {
    return false;
  } finally {
    if (activeTemplateWrite === write) activeTemplateWrite = null;
  }
}

function scheduleTemplateAutosave() {
  if (suppressAutosave || screen.value !== "templates") return;
  templateAutosaveStatus.value = "dirty";
  if (pendingTemplateDraft.value) return;
  if (templateAutosaveTimer) clearTimeout(templateAutosaveTimer);
  templateAutosaveTimer = setTimeout(() => {
    void persistTemplateDraft();
  }, AUTOSAVE_DELAY_MS);
}

async function loadTemplateDraft() {
  const draft = await journal.getDraft<TemplateEditorDraftPayload>(TEMPLATE_DRAFT_KEY);
  if (draft && hasTemplateContent(draft.payload)) {
    pendingTemplateDraft.value = draft;
  }
}

async function restoreTemplateDraft() {
  const draft = pendingTemplateDraft.value;
  if (!draft) return;
  suppressAutosave = true;
  tplName.value = draft.payload.name;
  tplBody.value = draft.payload.body_md;
  tplFieldsJson.value = draft.payload.fields_json;
  await nextTick();
  lastTemplateSignature = templateSignature();
  pendingTemplateDraft.value = null;
  templateAutosaveStatus.value = "saved";
  suppressAutosave = false;
}

async function discardTemplateDraft() {
  await journal.deleteDraft(TEMPLATE_DRAFT_KEY);
  pendingTemplateDraft.value = null;
  lastTemplateSignature = templateSignature();
  templateAutosaveStatus.value = "idle";
}

async function persistDraftNow(): Promise<boolean> {
  if (autosaveTimer) {
    clearTimeout(autosaveTimer);
    autosaveTimer = null;
  }
  if (screen.value !== "edit" || pendingDraft.value) return true;

  if (activeDraftWrite) await activeDraftWrite;
  const payload = draftPayload();
  const signature = payloadSignature(payload);
  if (signature === lastDraftSignature) return true;
  if (!hasDraftContent(payload) && !editingId.value) {
    autosaveStatus.value = "idle";
    lastDraftSignature = signature;
    return true;
  }

  const key = draftKey();
  autosaveStatus.value = "saving";
  autosaveError.value = "";
  const write = (async () => {
    try {
      await journal.saveDraft({
        draft_key: key,
        entry_id: editingId.value,
        payload,
        base_updated_at: baseUpdatedAt.value,
      });
      lastDraftSignature = signature;
      if (payloadSignature() === signature) {
        autosaveStatus.value = "saved";
      } else {
        autosaveStatus.value = "dirty";
        scheduleAutosave();
      }
    } catch (e) {
      autosaveError.value = String(e);
      autosaveStatus.value = "error";
      throw e;
    }
  })();
  activeDraftWrite = write;
  try {
    await write;
    return true;
  } catch {
    return false;
  } finally {
    if (activeDraftWrite === write) activeDraftWrite = null;
  }
}

function scheduleAutosave() {
  if (suppressAutosave || screen.value !== "edit") return;
  autosaveStatus.value = "dirty";
  if (pendingDraft.value) return;
  if (autosaveTimer) clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(() => {
    void persistDraftNow();
  }, AUTOSAVE_DELAY_MS);
}

async function flushDraft(): Promise<boolean> {
  if (screen.value !== "edit" || pendingDraft.value) return true;
  return persistDraftNow();
}

async function findRecoverableDraft(entry: JournalEntry | null) {
  const draft = await journal.getDraft(draftKey());
  if (!draft || !hasDraftContent(draft.payload)) return;
  if (entry && !isRecoverableDraft(draft.updated_at, entry.updated_at)) {
    await journal.deleteDraft(draft.draft_key);
    return;
  }
  pendingDraft.value = draft;
}

async function scanRecoverableDrafts() {
  const drafts = await journal.listDrafts();
  const recoverable: JournalDraft[] = [];
  for (const raw of drafts) {
    if (raw.draft_key === TEMPLATE_DRAFT_KEY) continue;
    const draft = raw as JournalDraft;
    if (!hasDraftContent(draft.payload)) continue;
    const entry = journal.entries.find((item) => item.id === draft.entry_id);
    if (isRecoverableDraft(draft.updated_at, entry?.updated_at)) {
      recoverable.push(draft);
    } else {
      await journal.deleteDraft(draft.draft_key);
    }
  }
  recoverableDrafts.value = recoverable;
}

function draftDisplayTitle(draft: JournalDraft) {
  if (!draft.entry_id) return draft.payload.title || "Новая запись";
  return (
    journal.entries.find((entry) => entry.id === draft.entry_id)?.title ||
    draft.payload.title ||
    "Запись журнала"
  );
}

async function openRecoverableDraft(draft: JournalDraft) {
  if (draft.entry_id) {
    await openEntry(draft.entry_id);
  } else {
    await startNew();
  }
}

function applyPayload(payload: JournalDraftPayload) {
  title.value = payload.title;
  selectedTemplateId.value = payload.template_id;
  bodyMd.value = payload.body_md;
  fieldValues.value = { ...payload.fields };
  tagsStr.value = payload.tags;
  entryDate.value = payload.entry_date;
}

async function restoreDraft() {
  const draft = pendingDraft.value;
  if (!draft) return;
  suppressAutosave = true;
  applyPayload(draft.payload);
  await nextTick();
  lastDraftSignature = payloadSignature();
  pendingDraft.value = null;
  autosaveStatus.value = "saved";
  suppressAutosave = false;
}

async function discardDraft() {
  const draft = pendingDraft.value;
  if (!draft) return;
  await journal.deleteDraft(draft.draft_key);
  pendingDraft.value = null;
  recoverableDrafts.value = recoverableDrafts.value.filter(
    (item) => item.draft_key !== draft.draft_key,
  );
  lastDraftSignature = payloadSignature();
  autosaveStatus.value = "idle";
}

function onVisibilityChange() {
  if (document.visibilityState === "hidden") {
    void flushDraft();
    void persistTemplateDraft();
  }
}

function onWindowBlur() {
  void flushDraft();
  void persistTemplateDraft();
}

onMounted(async () => {
  await journal.refresh();
  await scanRecoverableDrafts();
  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("blur", onWindowBlur);
  try {
    unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
      const journalDirty =
        screen.value === "edit" && payloadSignature() !== lastDraftSignature;
      const templateDirty =
        screen.value === "templates" && templateSignature() !== lastTemplateSignature;
      await persistDraftsBeforeClose({
        journalDirty,
        templateDirty,
        preventClose: () => event.preventDefault(),
        flushJournal: flushDraft,
        flushTemplate: persistTemplateDraft,
        destroyWindow: () => getCurrentWindow().destroy(),
      });
    });
  } catch {
    // Browser-only UI development has no Tauri window. Debounce, visibility
    // and route guards still protect the draft there.
  }
});

function parseTags() {
  return tagsStr.value
    .split(/[,#]+/)
    .map((t) => t.trim())
    .filter(Boolean);
}

async function startNew(tpl?: TemplateRecord) {
  const [journalOk, templateOk] = await Promise.all([
    flushDraft(),
    persistTemplateDraft(),
  ]);
  if (!journalOk || !templateOk) return;
  suppressAutosave = true;
  editingId.value = null;
  baseUpdatedAt.value = null;
  pendingDraft.value = null;
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
  await nextTick();
  lastDraftSignature = payloadSignature();
  autosaveStatus.value = "idle";
  await findRecoverableDraft(null);
  suppressAutosave = false;
}

async function openEntry(id: string) {
  if (!(await flushDraft())) return;
  const e = journal.entries.find((x) => x.id === id);
  if (!e) return;
  suppressAutosave = true;
  editingId.value = e.id;
  baseUpdatedAt.value = e.updated_at;
  pendingDraft.value = null;
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
  await nextTick();
  lastDraftSignature = payloadSignature();
  autosaveStatus.value = "idle";
  await findRecoverableDraft(e);
  suppressAutosave = false;
}

async function saveEntry(): Promise<boolean> {
  await flushDraft();
  const oldDraftKey = draftKey();
  const payload = {
    title: title.value,
    template_id: selectedTemplateId.value,
    body_md: bodyMd.value,
    fields: { ...fieldValues.value },
    tags: parseTags(),
    entry_date: entryDate.value,
  };
  if (editingId.value) {
    const e = await journal.update(editingId.value, payload);
    if (!e) return false;
    baseUpdatedAt.value = e.updated_at;
  } else {
    const e = await journal.create(payload);
    if (!e) return false;
    editingId.value = e.id;
    baseUpdatedAt.value = e.updated_at;
  }
  await Promise.allSettled([
    journal.deleteDraft(oldDraftKey),
    ...(oldDraftKey !== draftKey() ? [journal.deleteDraft(draftKey())] : []),
  ]);
  recoverableDrafts.value = recoverableDrafts.value.filter(
    (draft) => draft.draft_key !== oldDraftKey && draft.draft_key !== draftKey(),
  );
  lastDraftSignature = payloadSignature();
  autosaveStatus.value = "saved";
  return true;
}

async function showScreen(next: typeof screen.value) {
  if (screen.value === "edit" && next !== "edit" && !(await flushDraft())) return;
  if (
    screen.value === "templates" &&
    next !== "templates" &&
    !(await persistTemplateDraft())
  ) {
    return;
  }
  screen.value = next;
  if (next !== "preview") journal.clearPreview();
  if (next === "list") await scanRecoverableDrafts();
  if (next === "templates") {
    lastTemplateSignature = templateSignature();
    templateAutosaveStatus.value = "idle";
    await loadTemplateDraft();
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
  if (!(await saveEntry())) return;
  if (!editingId.value) return;
  await journal.previewExport(editingId.value);
  screen.value = "preview";
}

async function saveAsTemplate() {
  if (!(await saveEntry())) return;
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
  const created = await journal.createTemplate({
    name: tplName.value,
    body_md: tplBody.value,
    fields,
    category: "custom",
  });
  if (!created) return;
  await journal.deleteDraft(TEMPLATE_DRAFT_KEY);
  tplName.value = "";
  tplBody.value = "";
  tplFieldsJson.value = "[]";
  lastTemplateSignature = templateSignature();
  pendingTemplateDraft.value = null;
  templateAutosaveStatus.value = "saved";
  screen.value = "list";
}

const previewHtml = computed(() =>
  journal.preview ? renderMarkdown(journal.preview.markdown) : "",
);

watch(
  [title, bodyMd, entryDate, tagsStr, selectedTemplateId, fieldValues],
  scheduleAutosave,
  { deep: true },
);

watch([tplName, tplBody, tplFieldsJson], scheduleTemplateAutosave);

onBeforeRouteLeave(async () => {
  const [journalOk, templateOk] = await Promise.all([
    flushDraft(),
    persistTemplateDraft(),
  ]);
  return journalOk && templateOk;
});

onBeforeUnmount(() => {
  if (autosaveTimer) clearTimeout(autosaveTimer);
  if (templateAutosaveTimer) clearTimeout(templateAutosaveTimer);
  void flushDraft();
  void persistTemplateDraft();
  document.removeEventListener("visibilitychange", onVisibilityChange);
  window.removeEventListener("blur", onWindowBlur);
  unlistenClose?.();
});
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Лабораторный журнал</h1>
      <div class="toolbar">
        <button v-if="screen !== 'list'" class="btn" @click="showScreen('list')">
          ← К списку
        </button>
        <button class="btn" @click="showScreen('templates')">Шаблоны</button>
        <button class="btn btn-primary" @click="startNew()">Новая запись</button>
      </div>
    </header>

    <!-- LIST -->
    <div v-if="screen === 'list'" class="page-body">
      <div
        v-if="recoverableDrafts.length"
        class="draft-recovery"
        role="status"
        style="margin-bottom: 16px; align-items: flex-start"
      >
        <div style="flex: 1">
          <strong>Есть несохранённые черновики</strong>
          <div
            v-for="draft in recoverableDrafts"
            :key="draft.draft_key"
            class="draft-list-row"
          >
            <span>
              {{ draftDisplayTitle(draft) }}
              <span class="muted" style="font-size: 0.8rem">
                · {{ new Date(draft.updated_at).toLocaleString() }}
              </span>
            </span>
            <button class="btn btn-primary" @click="openRecoverableDraft(draft)">
              Открыть
            </button>
          </div>
        </div>
      </div>
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
        <div v-if="pendingDraft" class="draft-recovery" role="status">
          <div>
            <strong>Найден несохранённый черновик</strong>
            <div class="muted" style="font-size: 0.82rem">
              {{ new Date(pendingDraft.updated_at).toLocaleString() }}
            </div>
          </div>
          <div class="toolbar">
            <button class="btn btn-primary" @click="restoreDraft">Восстановить</button>
            <button class="btn" @click="discardDraft">Удалить черновик</button>
          </div>
        </div>
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
          <span
            v-if="autosaveLabel"
            class="autosave-status"
            :class="`is-${autosaveStatus}`"
            aria-live="polite"
          >
            {{ autosaveLabel }}
          </span>
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
      <div v-if="pendingTemplateDraft" class="draft-recovery" role="status" style="margin-bottom: 12px">
        <div>
          <strong>Найден черновик шаблона</strong>
          <div class="muted" style="font-size: 0.82rem">
            {{ new Date(pendingTemplateDraft.updated_at).toLocaleString() }}
          </div>
        </div>
        <div class="toolbar">
          <button class="btn btn-primary" @click="restoreTemplateDraft">Восстановить</button>
          <button class="btn" @click="discardTemplateDraft">Удалить</button>
        </div>
      </div>
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
        <div class="toolbar">
          <button class="btn btn-primary" @click="createCustomTemplate">Создать шаблон</button>
          <span
            v-if="templateAutosaveLabel"
            class="autosave-status"
            :class="`is-${templateAutosaveStatus}`"
            aria-live="polite"
          >
            {{ templateAutosaveLabel }}
          </span>
        </div>
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
.draft-recovery {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border: 1px solid color-mix(in srgb, var(--accent) 50%, var(--border));
  border-radius: 8px;
  background: color-mix(in srgb, var(--accent) 10%, var(--bg));
}
.draft-list-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
}
.autosave-status {
  margin-left: auto;
  color: var(--muted);
  font-size: 0.82rem;
}
.autosave-status.is-error {
  color: var(--danger);
}
.autosave-status.is-saved {
  color: var(--success, var(--accent));
}
label {
  display: block;
}
</style>
