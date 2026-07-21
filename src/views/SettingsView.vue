<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../stores/app";

interface CollabStatus {
  running: boolean;
  port: number | null;
  url: string | null;
  message: string;
}

const app = useAppStore();
const chromaPath = ref("");
const saved = ref(false);
const collab = ref<CollabStatus | null>(null);
const collabPort = ref(8765);
const collabBusy = ref(false);

onMounted(async () => {
  await app.loadInfo();
  try {
    const v = await invoke<string | null>("get_setting", { key: "chroma_path" });
    chromaPath.value = v || "";
  } catch {
    /* ignore */
  }
  try {
    collab.value = await invoke<CollabStatus>("collab_status");
  } catch {
    /* ignore */
  }
});

async function saveChroma() {
  await invoke("set_setting", { key: "chroma_path", value: chromaPath.value });
  saved.value = true;
  setTimeout(() => (saved.value = false), 1500);
}

async function startCollab() {
  collabBusy.value = true;
  try {
    collab.value = await invoke<CollabStatus>("collab_start", { port: collabPort.value });
  } catch (e) {
    app.setError(String(e));
  } finally {
    collabBusy.value = false;
  }
}

async function stopCollab() {
  collabBusy.value = true;
  try {
    collab.value = await invoke<CollabStatus>("collab_stop");
  } catch (e) {
    app.setError(String(e));
  } finally {
    collabBusy.value = false;
  }
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Настройки</h1>
    </header>
    <div class="page-body" style="max-width: 640px; display: flex; flex-direction: column; gap: 16px">
      <div class="card">
        <h3 style="margin: 0 0 8px">Приложение</h3>
        <p class="muted" style="margin: 0">
          {{ app.info?.name || "SoheiDesk" }} v{{ app.info?.version || "…" }}
        </p>
        <p class="muted" style="margin: 8px 0 0; font-size: 0.8rem; font-family: var(--mono); word-break: break-all">
          data: {{ app.info?.data_dir || "…" }}
        </p>
      </div>

      <div class="card">
        <h3 style="margin: 0 0 8px">Тема</h3>
        <div class="toolbar">
          <button class="btn" :class="{ 'btn-primary': app.theme === 'system' }" @click="app.setTheme('system')">
            System
          </button>
          <button class="btn" :class="{ 'btn-primary': app.theme === 'dark' }" @click="app.setTheme('dark')">
            Dark
          </button>
          <button class="btn" :class="{ 'btn-primary': app.theme === 'light' }" @click="app.setTheme('light')">
            Light
          </button>
        </div>
      </div>

      <div class="card">
        <h3 style="margin: 0 0 8px">LAN share (коллаб)</h3>
        <p class="muted" style="margin: 0 0 10px; font-size: 0.9rem">
          Read-only HTTP на локальной сети: журнал и библиография. Не для интернета.
        </p>
        <div class="toolbar" style="margin-bottom: 8px">
          <label class="muted" style="font-size: 0.85rem">
            Порт
            <input v-model.number="collabPort" type="number" class="input" style="width: 100px; margin-left: 6px" />
          </label>
          <button class="btn btn-primary" :disabled="collabBusy || collab?.running" @click="startCollab">
            Старт
          </button>
          <button class="btn" :disabled="collabBusy || !collab?.running" @click="stopCollab">Стоп</button>
        </div>
        <p style="margin: 0; font-size: 0.9rem">{{ collab?.message || "…" }}</p>
        <p v-if="collab?.url" class="muted" style="margin: 6px 0 0; font-family: var(--mono); font-size: 0.85rem">
          {{ collab.url }}
        </p>
      </div>

      <div class="card">
        <h3 style="margin: 0 0 8px">ChromaTsvet (опционально)</h3>
        <p class="muted" style="margin: 0 0 10px; font-size: 0.9rem">
          Путь к binary для открытия спектров из журнала.
        </p>
        <input v-model="chromaPath" type="text" class="input" placeholder="/path/to/chromattsvet" style="width: 100%; margin-bottom: 10px" />
        <button class="btn btn-primary" @click="saveChroma">
          {{ saved ? "Сохранено" : "Сохранить" }}
        </button>
      </div>

      <div class="card">
        <h3 style="margin: 0 0 8px">Горячие клавиши</h3>
        <ul class="muted" style="margin: 0; padding-left: 1.2rem; font-size: 0.9rem; line-height: 1.7">
          <li><kbd>⌘/Ctrl</kbd>+<kbd>O</kbd> — открыть файл</li>
          <li><kbd>⌘/Ctrl</kbd>+<kbd>F</kbd> — поиск (экран Поиск)</li>
          <li><kbd>⌘/Ctrl</kbd>+<kbd>J</kbd> — журнал</li>
          <li><kbd>⌘/Ctrl</kbd>+<kbd>E</kbd> — экспорт</li>
          <li><kbd>⌘/Ctrl</kbd>+<kbd>,</kbd> — настройки</li>
          <li><kbd>?</kbd> — эта справка (настройки)</li>
        </ul>
      </div>

      <div class="card">
        <h3 style="margin: 0 0 8px">Безопасность / FS</h3>
        <p class="muted" style="margin: 0; font-size: 0.9rem">
          Файлы только через dialog. LAN share — только чтение, bind 0.0.0.0 на выбранный порт.
        </p>
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
kbd {
  font-family: var(--mono);
  font-size: 0.8em;
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 1px 5px;
  background: var(--bg);
}
</style>
