<script setup lang="ts">
import { watch } from "vue";
import { useLibraryStore } from "../stores/library";
import { useAnnotationsStore } from "../stores/annotations";
import ReflowViewer from "../components/reader/ReflowViewer.vue";
import PdfViewer from "../components/reader/PdfViewer.vue";
import AnnotationPanel from "../components/reader/AnnotationPanel.vue";

const library = useLibraryStore();
const annotations = useAnnotationsStore();

watch(
  () => library.current?.document.id,
  (id) => {
    if (id) annotations.load(id);
    else annotations.clear();
  },
);

function formatSize(n: number | null | undefined) {
  if (n == null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Ридер</h1>
      <div class="toolbar">
        <button class="btn btn-primary" :disabled="library.loading" @click="library.openViaDialog()">
          {{ library.loading ? "Открытие…" : "Открыть файл…" }}
        </button>
        <button v-if="library.current" class="btn" @click="library.clearCurrent(); annotations.clear()">
          Закрыть
        </button>
      </div>
    </header>

    <div v-if="!library.current" class="empty">
      <div>
        <p style="font-size: 1.1rem; color: var(--text); margin: 0 0 8px">Нет открытого документа</p>
        <p class="muted" style="margin: 0 0 16px">
          PDF, MD, TXT, DOCX, EPUB, HTML, TEX — dialog only. ⌘/Ctrl+O
        </p>
        <button class="btn btn-primary" @click="library.openViaDialog()">Открыть файл…</button>
      </div>
    </div>

    <div v-else class="reader-layout">
      <div class="viewer">
        <div class="viewer-meta">
          <span><strong>{{ library.current.opened.title }}</strong></span>
          <span class="badge">{{ library.current.opened.doc_type }}</span>
          <span>{{ formatSize(library.current.opened.file_size) }}</span>
          <span title="content hash">hash: {{ library.current.opened.content_hash.slice(0, 12) }}…</span>
        </div>

        <ReflowViewer
          v-if="library.current.opened.doc_type === 'txt'"
          mode="txt"
          :text="library.current.opened.text || ''"
          :document-id="library.current.document.id"
        />
        <ReflowViewer
          v-else-if="['md', 'docx', 'epub', 'html', 'tex'].includes(library.current.opened.doc_type)"
          :mode="library.current.opened.doc_type === 'tex' ? 'txt' : 'md'"
          :text="library.current.opened.text || ''"
          :document-id="library.current.document.id"
        />
        <PdfViewer
          v-else-if="library.current.opened.doc_type === 'pdf'"
          :path="library.current.opened.path"
          :title="library.current.opened.title"
          :document-id="library.current.document.id"
        />
      </div>
      <AnnotationPanel />
    </div>
  </div>
</template>

<style scoped>
.reader-layout {
  flex: 1;
  min-height: 0;
  display: flex;
}
.viewer {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
</style>
