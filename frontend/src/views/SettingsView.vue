<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";
import { useUiModeStore } from "../stores/uiMode";
import type { BackupInfo, BackupKind, BackupRestoreResult } from "../types";

interface CollabStatus {
  running: boolean;
  port: number | null;
  url: string | null;
  message: string;
}

const app = useAppStore();
const ui = useUiModeStore();
const chromaPath = ref("");
const saved = ref(false);
const collab = ref<CollabStatus | null>(null);
const collabPort = ref(8765);
const collabBusy = ref(false);
const backups = ref<BackupInfo[]>([]);
const backupsVisible = ref(false);
const backupBusy = ref(false);
const backupMessage = ref("");

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

function backupKindLabel(kind: BackupKind) {
  return {
    daily: "Ежедневная",
    manual: "Ручная",
    pre_migration: "Перед миграцией",
    emergency: "Аварийная",
    unknown: "Неизвестная",
  }[kind];
}

function formatBackupDate(value: string) {
  if (!value) return "дата неизвестна";
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat("ru-RU", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} Б`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} КБ`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} МБ`;
  return `${(value / 1024 ** 3).toFixed(1)} ГБ`;
}

async function loadBackups() {
  backups.value = await invoke<BackupInfo[]>("list_backups");
}

async function toggleBackups() {
  backupsVisible.value = !backupsVisible.value;
  if (!backupsVisible.value) return;
  backupBusy.value = true;
  backupMessage.value = "";
  try {
    await loadBackups();
  } catch (e) {
    app.setError(String(e));
  } finally {
    backupBusy.value = false;
  }
}

async function createBackup() {
  backupBusy.value = true;
  backupMessage.value = "Создаём и проверяем копию…";
  try {
    const created = await invoke<BackupInfo>("create_backup");
    backupMessage.value = `Копия создана и проверена: ${created.file_name}`;
    if (backupsVisible.value) await loadBackups();
  } catch (e) {
    backupMessage.value = "";
    app.setError(String(e));
  } finally {
    backupBusy.value = false;
  }
}

async function restoreBackup(backup: BackupInfo) {
  if (!backup.readable) return;
  const approved = await confirm(
    `Восстановить данные из копии «${backup.file_name}»?\n\nТекущие данные сначала будут сохранены в аварийную копию.`,
    { title: "Восстановление SoheiDesk", kind: "warning" },
  );
  if (!approved) return;

  backupBusy.value = true;
  backupMessage.value = "Проверяем хеши и целостность SQLite…";
  try {
    const result = await invoke<BackupRestoreResult>("restore_backup", { backupId: backup.id });
    backupMessage.value = result.warning
      ? `Данные восстановлены. ${result.warning}`
      : `Данные восстановлены. Аварийная копия: ${result.emergency.file_name}. Перезапуск…`;
    window.setTimeout(() => window.location.reload(), 1400);
  } catch (e) {
    backupMessage.value = "Восстановление отменено или откат выполнен.";
    app.setError(String(e));
  } finally {
    backupBusy.value = false;
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
        <h3 style="margin: 0 0 8px">Режим интерфейса</h3>
        <p class="muted" style="margin: 0 0 10px; font-size: 0.9rem">
          Простой — как терминал (текст, мало кнопок). Обычный — полный ридер с PDF-страницами.
        </p>
        <div class="toolbar">
          <button class="btn" :class="{ 'btn-primary': ui.mode === 'simple' }" @click="ui.setMode('simple')">
            Простой
          </button>
          <button class="btn" :class="{ 'btn-primary': ui.mode === 'normal' }" @click="ui.setMode('normal')">
            Обычный
          </button>
          <button class="btn" @click="ui.resetMode()">Спросить при запуске</button>
        </div>
      </div>

      <div class="card">
        <h3 style="margin: 0 0 8px">Резервные копии</h3>
        <p class="muted" style="margin: 0 0 10px; font-size: 0.9rem">
          SoheiDesk делает копию раз в сутки и перед миграциями. Хранятся 7 последних дневных и
          4 более старые недельные копии. Ручные и аварийные копии автоматически не удаляются.
        </p>
        <div class="toolbar">
          <button class="btn btn-primary" :disabled="backupBusy" @click="createBackup">
            Создать копию
          </button>
          <button class="btn" :disabled="backupBusy" @click="toggleBackups">
            {{ backupsVisible ? "Скрыть копии" : "Показать копии" }}
          </button>
        </div>
        <p v-if="backupMessage" style="margin: 10px 0 0; font-size: 0.88rem">
          {{ backupMessage }}
        </p>
        <p class="muted" style="margin: 10px 0 0; font-size: 0.8rem">
          Перед восстановлением автоматически проверяются SHA-256 всех файлов и SQLite integrity_check.
          Поисковый индекс не копируется — он пересоздаётся после восстановления.
        </p>

        <div v-if="backupsVisible" class="backup-list">
          <p v-if="backupBusy && backups.length === 0" class="muted">Загрузка…</p>
          <p v-else-if="backups.length === 0" class="muted">Копий пока нет.</p>
          <div v-for="backup in backups" :key="`${backup.file_name}-${backup.id}`" class="backup-row">
            <div style="min-width: 0; flex: 1">
              <strong>{{ backupKindLabel(backup.kind) }}</strong>
              <div class="muted backup-meta">
                {{ formatBackupDate(backup.created_at) }} · {{ formatBytes(backup.size_bytes) }}
                <template v-if="backup.schema_version >= 0"> · DB v{{ backup.schema_version }}</template>
              </div>
              <div class="muted backup-file">{{ backup.file_name }}</div>
              <div v-if="!backup.readable" class="backup-error">
                Архив повреждён или имеет неизвестный формат: {{ backup.error }}
              </div>
            </div>
            <button
              class="btn"
              :disabled="backupBusy || !backup.readable"
              @click="restoreBackup(backup)"
            >
              Восстановить
            </button>
          </div>
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
.backup-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 12px;
}
.backup-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
}
.backup-meta {
  margin-top: 3px;
  font-size: 0.82rem;
}
.backup-file {
  overflow: hidden;
  margin-top: 3px;
  font-family: var(--mono);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.backup-error {
  margin-top: 5px;
  color: var(--danger, #c2413b);
  font-size: 0.8rem;
}
</style>
