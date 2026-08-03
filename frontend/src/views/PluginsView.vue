<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";

interface Plugin {
  id: string;
  name: string;
  command: string;
  args_json: string | null;
  extensions_json: string;
  description: string | null;
  enabled: boolean;
}

const app = useAppStore();
const plugins = ref<Plugin[]>([]);
const name = ref("");
const command = ref("");
const args = ref("{path}");
const extensions = ref("odt");
const description = ref("");
const output = ref("");
const loading = ref(false);

async function refresh() {
  plugins.value = await invoke<Plugin[]>("list_plugins");
}

onMounted(async () => {
  try {
    await refresh();
  } catch (e) {
    app.setError(String(e));
  }
});

async function create() {
  loading.value = true;
  try {
    const argsList = args.value
      .split(/\s+/)
      .map((s) => s.trim())
      .filter(Boolean);
    const exts = extensions.value
      .split(/[,\s]+/)
      .map((s) => s.trim().replace(/^\./, ""))
      .filter(Boolean);
    await invoke("create_plugin", {
      input: {
        name: name.value,
        command: command.value,
        args: argsList,
        extensions: exts,
        description: description.value || null,
        enabled: true,
      },
    });
    name.value = "";
    command.value = "";
    args.value = "{path}";
    extensions.value = "";
    description.value = "";
    await refresh();
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function toggle(p: Plugin) {
  await invoke("set_plugin_enabled", { id: p.id, enabled: !p.enabled });
  await refresh();
}

async function remove(id: string) {
  await invoke("delete_plugin", { id });
  await refresh();
}

async function run(p: Plugin) {
  const path = await open({ multiple: false });
  if (typeof path !== "string") return;
  loading.value = true;
  try {
    output.value = await invoke<string>("run_plugin", {
      pluginId: p.id,
      filePath: path,
    });
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

function exts(p: Plugin) {
  try {
    return (JSON.parse(p.extensions_json) as string[]).join(", ");
  } catch {
    return p.extensions_json;
  }
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Плагины-парсеры</h1>
    </header>
    <div class="page-body" style="max-width: 900px; display: flex; flex-direction: column; gap: 16px">
      <div class="card">
        <p class="muted" style="margin: 0 0 10px; font-size: 0.9rem">
          Внешняя команда получает путь к файлу (плейсхолдер <code>{path}</code>) и печатает текст в stdout.
          Пример: <code>pandoc {path} -t markdown</code> для .odt
        </p>
        <div class="grid">
          <input v-model="name" class="input" placeholder="Имя" />
          <input v-model="command" class="input" placeholder="Команда (pandoc)" />
          <input v-model="args" class="input" placeholder="Аргументы: {path} -t markdown" />
          <input v-model="extensions" class="input" placeholder="Расширения: odt, rtf" />
          <input v-model="description" class="input" placeholder="Описание" style="grid-column: 1 / -1" />
        </div>
        <button class="btn btn-primary" style="margin-top: 10px" :disabled="loading" @click="create">
          Добавить плагин
        </button>
      </div>

      <div class="list">
        <div v-for="p in plugins" :key="p.id" class="list-item">
          <div style="min-width: 0; flex: 1">
            <strong>{{ p.name }}</strong>
            <span class="badge" style="margin-left: 8px">.{{ exts(p) }}</span>
            <span v-if="!p.enabled" class="badge" style="margin-left: 4px">off</span>
            <div class="muted" style="font-size: 0.8rem; font-family: var(--mono)">
              {{ p.command }} {{ p.args_json }}
            </div>
            <div v-if="p.description" class="muted" style="font-size: 0.85rem">{{ p.description }}</div>
          </div>
          <div class="toolbar">
            <button class="btn" @click="run(p)">Запуск…</button>
            <button class="btn" @click="toggle(p)">{{ p.enabled ? "Выкл" : "Вкл" }}</button>
            <button class="btn btn-danger" @click="remove(p.id)">×</button>
          </div>
        </div>
        <div v-if="plugins.length === 0" class="muted">Нет плагинов.</div>
      </div>

      <div v-if="output" class="card">
        <div class="muted" style="margin-bottom: 6px">Вывод плагина</div>
        <pre class="mono">{{ output }}</pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.grid {
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
.mono {
  white-space: pre-wrap;
  font-family: var(--mono);
  font-size: 0.85rem;
  margin: 0;
  max-height: 320px;
  overflow: auto;
}
</style>
