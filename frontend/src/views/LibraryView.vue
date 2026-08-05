<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRouter } from "vue-router";
import { useLibraryStore } from "../stores/library";
import { useAppStore } from "../stores/app";
import type { DocumentVersion } from "../types";

const library = useLibraryStore();
const router = useRouter();
const app = useAppStore();
const versions = ref<Record<string, DocumentVersion[]>>({});
const expandedDocument = ref<string | null>(null);

onMounted(() => {
  library.refresh();
});

async function openDoc(id: string) {
  await library.openById(id);
  if (library.current) {
    router.push("/reader");
  }
}

const changeLabels: Record<string, string> = {
  added: "Добавлен",
  verified: "Проверена идентичность",
  moved: "Перемещён",
  alternate_path: "Открыт из другого места",
  content_changed: "Содержимое изменено",
  imported: "Импортирован",
};

async function toggleVersions(documentId: string) {
  if (expandedDocument.value === documentId) {
    expandedDocument.value = null;
    return;
  }
  if (!versions.value[documentId]) {
    try {
      versions.value[documentId] = await invoke<DocumentVersion[]>("list_document_versions", {
        documentId,
      });
    } catch (error) {
      app.setError(String(error));
      return;
    }
  }
  expandedDocument.value = documentId;
}

function formatDate(value: string) {
  return new Date(value).toLocaleString();
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Библиотека</h1>
      <div class="toolbar">
        <button class="btn btn-primary" @click="library.openViaDialog()">Добавить…</button>
        <button class="btn" @click="library.refresh()">Обновить</button>
      </div>
    </header>

    <div class="page-body">
      <div v-if="library.documents.length === 0" class="empty" style="min-height: 240px">
        <div>
          <p style="color: var(--text)">Библиотека пуста</p>
          <p class="muted">Откройте PDF / MD / TXT — документ получит полный SHA-256 и историю версий.</p>
        </div>
      </div>

      <div v-else class="list">
        <div v-for="doc in library.documents" :key="doc.id" class="document-card">
          <div class="list-item">
            <div style="min-width: 0; flex: 1">
              <div style="display: flex; gap: 8px; align-items: center; margin-bottom: 4px">
                <strong style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">
                  {{ doc.title || "Без названия" }}
                </strong>
                <span class="badge">{{ doc.doc_type }}</span>
              </div>
              <div class="muted" style="font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis">
                {{ doc.last_path || "path unknown" }}
              </div>
              <div class="muted" style="font-size: 0.75rem; font-family: var(--mono); margin-top: 4px">
                {{ doc.sha256 ? "SHA-256" : "legacy" }}:
                {{ (doc.sha256 || doc.content_hash).slice(0, 16) }}…
              </div>
            </div>
            <div class="toolbar">
              <button class="btn btn-primary" @click="openDoc(doc.id)">Открыть</button>
              <button class="btn" @click="toggleVersions(doc.id)">
                История ({{ doc.version_count }})
              </button>
              <button class="btn btn-danger" @click="library.remove(doc.id)">Убрать</button>
            </div>
          </div>
          <div v-if="expandedDocument === doc.id" class="version-history">
            <div v-if="(versions[doc.id] || []).length === 0" class="muted">
              История появится после следующего открытия документа.
            </div>
            <div v-for="version in versions[doc.id] || []" :key="version.id" class="version-row">
              <div>
                <strong>{{ changeLabels[version.change_kind] || version.change_kind }}</strong>
                <span class="muted"> · {{ formatDate(version.observed_at) }}</span>
              </div>
              <div v-if="version.path" class="muted version-path">{{ version.path }}</div>
              <div class="muted version-hash">
                {{ version.sha256 ? "SHA-256" : "legacy" }}:
                {{ (version.sha256 || version.legacy_hash || "unknown").slice(0, 20) }}…
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.document-card {
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.document-card .list-item {
  border: 0;
  border-radius: 0;
}
.version-history {
  padding: 0 14px 12px;
  background: var(--bg-elevated);
}
.version-row {
  padding: 9px 0;
  border-top: 1px solid var(--border);
  font-size: 0.8rem;
}
.version-path {
  margin-top: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.version-hash {
  margin-top: 3px;
  font-family: var(--mono);
}
</style>
