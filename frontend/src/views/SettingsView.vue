<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";
import { useUiModeStore } from "../stores/uiMode";
import type {
  BackupInfo,
  BackupKind,
  BackupRestoreResult,
  WorkspaceCounts,
  WorkspaceExportResult,
  WorkspaceImportResult,
  WorkspacePreview,
} from "../types";

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
const workspaceBusy = ref(false);
const workspaceMessage = ref("");
const workspacePreview = ref<WorkspacePreview | null>(null);

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

function totalWorkspaceRecords(counts: WorkspaceCounts) {
  return Object.values(counts).reduce((total, value) => total + value, 0);
}

async function exportWorkspace() {
  const path = await save({
    defaultPath: `soheidesk-workspace-${new Date().toISOString().slice(0, 10)}.zip`,
    filters: [{ name: "SoheiDesk workspace", extensions: ["zip"] }],
  });
  if (!path) return;
  workspaceBusy.value = true;
  workspaceMessage.value = "Создаём и проверяем переносимый архив…";
  try {
    const result = await invoke<WorkspaceExportResult>("export_workspace", { path });
    workspaceMessage.value = result.missing_references.length
      ? `Архив создан. Не удалось включить ссылок: ${result.missing_references.length}.`
      : `Архив создан и проверен: ${result.file_count} файлов, ${formatBytes(result.total_size)}.`;
  } catch (error) {
    workspaceMessage.value = "";
    app.setError(String(error));
  } finally {
    workspaceBusy.value = false;
  }
}

async function selectWorkspaceImport() {
  const path = await open({
    multiple: false,
    filters: [{ name: "SoheiDesk workspace", extensions: ["zip"] }],
  });
  if (!path || Array.isArray(path)) return;
  workspaceBusy.value = true;
  workspaceMessage.value = "Проверяем структуру, SHA-256 и SQLite…";
  workspacePreview.value = null;
  try {
    workspacePreview.value = await invoke<WorkspacePreview>("preview_workspace_import", { path });
    workspaceMessage.value = "Архив проверен. Просмотрите состав перед импортом.";
  } catch (error) {
    workspaceMessage.value = "";
    app.setError(String(error));
  } finally {
    workspaceBusy.value = false;
  }
}

async function importWorkspace() {
  const preview = workspacePreview.value;
  if (!preview) return;
  const replacement = preview.requires_replacement_confirmation
    ? `\n\nТекущие ${totalWorkspaceRecords(preview.current_counts)} записей будут заменены после создания аварийной копии.`
    : "";
  const approved = await confirm(
    `Импортировать «${preview.file_name}»?${replacement}\n\nБез подтверждения существующие данные не перезаписываются.`,
    { title: "Импорт рабочего пространства", kind: "warning" },
  );
  if (!approved) return;
  workspaceBusy.value = true;
  workspaceMessage.value = "Создаём аварийную копию и импортируем данные…";
  try {
    const result = await invoke<WorkspaceImportResult>("import_workspace", {
      token: preview.token,
      replaceExisting: preview.requires_replacement_confirmation,
    });
    workspaceMessage.value = result.warning
      ? `Импорт завершён. ${result.warning}`
      : `Импорт завершён. Аварийная копия: ${result.emergency.file_name}. Перезапуск…`;
    window.setTimeout(() => window.location.reload(), 1400);
  } catch (error) {
    workspaceMessage.value = "Импорт не выполнен; текущие данные сохранены.";
    app.setError(String(error));
  } finally {
    workspaceBusy.value = false;
  }
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
        <h3 style="margin: 0 0 8px">Перенос рабочего пространства</h3>
        <p class="muted" style="margin: 0 0 10px; font-size: 0.9rem">
          Экспорт создаёт обычный ZIP с SQLite, вложениями, media, manifest и README. Архив можно
          читать без SoheiDesk. Перед импортом показывается состав, а текущие данные не заменяются
          без отдельного подтверждения.
        </p>
        <div class="toolbar">
          <button class="btn btn-primary" :disabled="workspaceBusy" @click="exportWorkspace">
            Экспортировать всё…
          </button>
          <button class="btn" :disabled="workspaceBusy" @click="selectWorkspaceImport">
            Проверить архив…
          </button>
        </div>
        <p v-if="workspaceMessage" style="margin: 10px 0 0; font-size: 0.88rem">
          {{ workspaceMessage }}
        </p>

        <div v-if="workspacePreview" class="workspace-preview">
          <div style="display: flex; justify-content: space-between; gap: 10px">
            <strong>{{ workspacePreview.file_name }}</strong>
            <span class="badge">
              {{ workspacePreview.compatibility === "compatible" ? "Совместим" : "Будет обновлён" }}
            </span>
          </div>
          <div class="muted backup-meta">
            DB v{{ workspacePreview.schema_version }} · SoheiDesk {{ workspacePreview.app_version }} ·
            {{ workspacePreview.file_count }} файлов · {{ formatBytes(workspacePreview.total_size) }}
          </div>
          <ul class="workspace-counts">
            <li>Документы: {{ workspacePreview.counts.documents }}</li>
            <li>Аннотации: {{ workspacePreview.counts.annotations }}</li>
            <li>Записи журнала: {{ workspacePreview.counts.journal_entries }}</li>
            <li>Вложения: {{ workspacePreview.attachment_count }}</li>
            <li>Media: {{ workspacePreview.media_count }}</li>
          </ul>
          <div v-if="workspacePreview.requires_replacement_confirmation" class="backup-error">
            Текущее рабочее пространство не пусто. Импорт потребует явного подтверждения замены и
            сначала создаст аварийную копию.
          </div>
          <details v-if="workspacePreview.missing_references.length" class="missing-files">
            <summary>
              Недоступные внешние файлы: {{ workspacePreview.missing_references.length }}
            </summary>
            <ul>
              <li v-for="item in workspacePreview.missing_references" :key="item">{{ item }}</li>
            </ul>
          </details>
          <button class="btn btn-primary" :disabled="workspaceBusy" @click="importWorkspace">
            Импортировать
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
.workspace-preview {
  margin-top: 12px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
}
.workspace-counts {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 16px;
  margin: 10px 0;
  padding-left: 18px;
  color: var(--text-muted);
  font-size: 0.82rem;
}
.missing-files {
  margin: 10px 0;
  color: var(--text-muted);
  font-size: 0.8rem;
}
.missing-files ul {
  max-height: 120px;
  overflow: auto;
  word-break: break-all;
}
</style>
