<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";
import { useLibraryStore } from "../stores/library";

interface ZoteroItem {
  key: string;
  title: string;
  item_type: string;
  authors: string;
  year: string | null;
  attachment_path: string | null;
}

const app = useAppStore();
const library = useLibraryStore();
const dbPath = ref("");
const items = ref<ZoteroItem[]>([]);
const selected = ref<Record<string, boolean>>({});
const loading = ref(false);

onMounted(async () => {
  try {
    const v = await invoke<string | null>("get_setting", { key: "zotero_db_path" });
    if (v) dbPath.value = v;
  } catch {
    /* */
  }
});

async function pickDb() {
  const p = await open({
    multiple: false,
    filters: [{ name: "SQLite", extensions: ["sqlite", "db"] }],
  });
  if (typeof p === "string") {
    dbPath.value = p;
    await invoke("zotero_save_db_path", { path: p });
  }
}

async function loadItems() {
  if (!dbPath.value) {
    app.setError("Укажите путь к zotero.sqlite");
    return;
  }
  loading.value = true;
  app.setError(null);
  try {
    await invoke("zotero_save_db_path", { path: dbPath.value });
    items.value = await invoke<ZoteroItem[]>("zotero_list_items", {
      dbPath: dbPath.value,
      limit: 150,
    });
    selected.value = {};
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function importSelected() {
  const paths = items.value
    .filter((i) => selected.value[i.key] && i.attachment_path)
    .map((i) => i.attachment_path!) ;
  if (paths.length === 0) {
    app.setError("Выберите элементы с attachment path");
    return;
  }
  loading.value = true;
  try {
    await invoke("zotero_import_paths", { paths });
    await library.refresh();
    alert(`Импортировано файлов: ${paths.length}`);
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
      <h1>Интеграции · Zotero</h1>
      <div class="toolbar">
        <button class="btn" @click="pickDb">Выбрать zotero.sqlite…</button>
        <button class="btn btn-primary" :disabled="loading" @click="loadItems">
          Загрузить список
        </button>
        <button class="btn" :disabled="loading" @click="importSelected">
          Импорт вложений
        </button>
      </div>
    </header>
    <div class="page-body">
      <div class="card" style="margin-bottom: 16px">
        <div class="muted" style="font-size: 0.85rem; margin-bottom: 6px">
          Локальная БД Zotero (обычно ~/Zotero/zotero.sqlite). Только чтение; файлы — через dialog-path.
        </div>
        <input v-model="dbPath" class="input" style="width: 100%" placeholder="/path/to/zotero.sqlite" />
      </div>

      <div v-if="items.length === 0" class="muted">Список пуст — загрузите БД.</div>
      <div v-else class="list">
        <div v-for="it in items" :key="it.key" class="list-item">
          <label style="display: flex; gap: 10px; align-items: flex-start; min-width: 0; flex: 1">
            <input v-model="selected[it.key]" type="checkbox" :disabled="!it.attachment_path" />
            <div style="min-width: 0">
              <strong>{{ it.title }}</strong>
              <div class="muted" style="font-size: 0.85rem">
                {{ it.authors }} · {{ it.year || "—" }} · {{ it.item_type }}
              </div>
              <div
                v-if="it.attachment_path"
                class="muted"
                style="font-size: 0.75rem; font-family: var(--mono); word-break: break-all"
              >
                {{ it.attachment_path }}
              </div>
              <div v-else class="muted" style="font-size: 0.75rem">нет локального файла</div>
            </div>
          </label>
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
