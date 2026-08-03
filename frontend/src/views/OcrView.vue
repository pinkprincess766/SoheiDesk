<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";

interface OcrStatus {
  available: boolean;
  version: string | null;
  message: string;
}

interface OcrResult {
  text: string;
  engine: string;
  note: string | null;
}

const app = useAppStore();
const status = ref<OcrStatus | null>(null);
const path = ref("");
const lang = ref("eng+rus");
const result = ref<OcrResult | null>(null);
const loading = ref(false);

onMounted(async () => {
  status.value = await invoke<OcrStatus>("ocr_status");
});

async function pickImage() {
  const p = await open({
    multiple: false,
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp"] }],
  });
  if (typeof p === "string") path.value = p;
}

async function runOcr() {
  if (!path.value) {
    app.setError("Выберите изображение");
    return;
  }
  loading.value = true;
  app.setError(null);
  try {
    result.value = await invoke<OcrResult>("ocr_image", {
      path: path.value,
      lang: lang.value || null,
    });
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
      <h1>OCR</h1>
    </header>
    <div class="page-body" style="max-width: 800px; display: flex; flex-direction: column; gap: 12px">
      <div class="card">
        <p style="margin: 0">
          {{ status?.message || "Проверка Tesseract…" }}
        </p>
        <p v-if="status?.version" class="muted" style="margin: 6px 0 0; font-family: var(--mono); font-size: 0.85rem">
          {{ status.version }}
        </p>
      </div>
      <div class="card" style="display: flex; flex-direction: column; gap: 10px">
        <div class="toolbar">
          <input v-model="path" class="input" style="flex: 1" readonly placeholder="путь к изображению" />
          <button class="btn" @click="pickImage">Выбрать…</button>
        </div>
        <label>
          <span class="muted" style="font-size: 0.8rem">Языки Tesseract (-l)</span>
          <input v-model="lang" class="input" style="width: 100%; margin-top: 4px" />
        </label>
        <button class="btn btn-primary" :disabled="loading || !status?.available" @click="runOcr">
          {{ loading ? "Распознавание…" : "OCR" }}
        </button>
      </div>
      <div v-if="result" class="card">
        <div class="muted" style="margin-bottom: 8px; font-size: 0.85rem">
          {{ result.engine }} · {{ result.note }}
        </div>
        <pre class="mono">{{ result.text }}</pre>
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
.mono {
  font-family: var(--mono);
  white-space: pre-wrap;
  margin: 0;
  font-size: 0.9rem;
}
</style>
