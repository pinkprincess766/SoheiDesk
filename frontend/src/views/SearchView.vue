<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { useSearchStore } from "../stores/search";
import { useLibraryStore } from "../stores/library";
import { useJournalStore } from "../stores/journal";

const search = useSearchStore();
const library = useLibraryStore();
const journal = useJournalStore();
const router = useRouter();
const inputRef = ref<HTMLInputElement | null>(null);

onMounted(() => {
  nextTick(() => inputRef.value?.focus());
});

async function openHit(hit: { id: string; kind: string }) {
  if (hit.kind === "document") {
    await library.openById(hit.id);
    router.push("/reader");
  } else if (hit.kind === "journal") {
    await journal.loadEntry(hit.id);
    router.push("/journal");
  }
}

async function reindex() {
  const n = await search.reindex();
  alert(`Индекс обновлён: ${n} документов`);
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Поиск</h1>
      <div class="toolbar">
        <button class="btn" @click="reindex">Переиндексировать</button>
      </div>
    </header>
    <div class="page-body" style="max-width: 800px">
      <div class="toolbar" style="margin-bottom: 16px">
        <input
          ref="inputRef"
          v-model="search.query"
          class="input"
          style="flex: 1"
          placeholder="Полнотекстовый поиск (Tantivy)…"
          @keydown.enter="search.run()"
        />
        <button class="btn btn-primary" :disabled="search.loading" @click="search.run()">
          {{ search.loading ? "…" : "Найти" }}
        </button>
      </div>

      <div v-if="search.hits.length === 0" class="muted">
        {{ search.query ? "Ничего не найдено" : "Введите запрос по библиотеке и журналу" }}
      </div>

      <div v-else class="list">
        <div
          v-for="h in search.hits"
          :key="h.kind + h.id"
          class="list-item"
          style="cursor: pointer"
          @click="openHit(h)"
        >
          <div style="min-width: 0">
            <div style="display: flex; gap: 8px; align-items: center">
              <strong>{{ h.title }}</strong>
              <span class="badge">{{ h.kind }}</span>
              <span class="muted" style="font-size: 0.75rem">{{ h.score.toFixed(2) }}</span>
            </div>
            <div class="muted" style="font-size: 0.85rem; margin-top: 4px">{{ h.snippet }}</div>
            <div v-if="h.path" class="muted" style="font-size: 0.75rem; font-family: var(--mono)">
              {{ h.path }}
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.input {
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
}
</style>
