<script setup lang="ts">
import { onMounted } from "vue";
import { useRouter } from "vue-router";
import { useLibraryStore } from "../stores/library";

const library = useLibraryStore();
const router = useRouter();

onMounted(() => {
  library.refresh();
});

async function openDoc(id: string) {
  await library.openById(id);
  if (library.current) {
    router.push("/reader");
  }
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
          <p class="muted">Откройте PDF / MD / TXT — документ появится здесь (identity по content_hash).</p>
        </div>
      </div>

      <div v-else class="list">
        <div v-for="doc in library.documents" :key="doc.id" class="list-item">
          <div style="min-width: 0">
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
              {{ doc.content_hash.slice(0, 16) }}…
            </div>
          </div>
          <div class="toolbar">
            <button class="btn btn-primary" @click="openDoc(doc.id)">Открыть</button>
            <button class="btn btn-danger" @click="library.remove(doc.id)">Убрать</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
