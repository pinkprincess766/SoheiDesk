<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import * as pdfjs from "pdfjs-dist";
import { useAppStore } from "../stores/app";
import type {
  BackupKind,
  DiagnosticArchiveResult,
  DiagnosticComponentState,
  DiagnosticComponentStatus,
  DiagnosticReport,
  DiagnosticStorageMetric,
} from "../types";

pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  "/pdf.worker.min.mjs",
  window.location.href,
).toString();

const app = useAppStore();
const report = ref<DiagnosticReport | null>(null);
const pdfWorker = ref<DiagnosticComponentStatus>({
  state: "not_checked",
  version: null,
  message: "Проверка ещё не запускалась.",
});
const loading = ref(false);
const exporting = ref(false);
const exportMessage = ref("");

async function probePdfWorker(): Promise<DiagnosticComponentStatus> {
  let worker: pdfjs.PDFWorker | null = null;
  let timeout: ReturnType<typeof setTimeout> | null = null;
  try {
    worker = new pdfjs.PDFWorker();
    await Promise.race([
      worker.promise,
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => reject(new Error("PDF worker timeout")), 4000);
      }),
    ]);
    return {
      state: "available",
      version: pdfjs.version || null,
      message: "PDF worker успешно выполнил handshake.",
    };
  } catch {
    return {
      state: "unavailable",
      version: null,
      message: "PDF worker не смог выполнить handshake.",
    };
  } finally {
    if (timeout) clearTimeout(timeout);
    worker?.destroy();
  }
}

async function refresh() {
  loading.value = true;
  exportMessage.value = "";
  try {
    const [snapshot, workerStatus] = await Promise.all([
      invoke<DiagnosticReport>("get_application_diagnostics"),
      probePdfWorker(),
    ]);
    pdfWorker.value = workerStatus;
    snapshot.components.pdf_worker = workerStatus;
    report.value = snapshot;
  } catch (error) {
    app.setError(String(error), "diagnostics.refresh");
  } finally {
    loading.value = false;
  }
}

async function exportArchive() {
  const path = await save({
    filters: [{ name: "SoheiDesk diagnostics", extensions: ["zip"] }],
    defaultPath: `soheidesk-diagnostics-${new Date().toISOString().slice(0, 10)}.zip`,
  });
  if (!path) return;
  exporting.value = true;
  exportMessage.value = "Создаём и проверяем архив…";
  try {
    const result = await invoke<DiagnosticArchiveResult>("export_diagnostic_archive", {
      path,
      pdfWorker: {
        state: pdfWorker.value.state,
        version: pdfWorker.value.version,
      },
    });
    exportMessage.value = `Архив проверен: ${result.file_name} (${formatBytes(result.size_bytes)})`;
  } catch (error) {
    exportMessage.value = "Экспорт не завершён.";
    app.setError(String(error), "diagnostics.export");
  } finally {
    exporting.value = false;
  }
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let amount = value / 1024;
  let unit = 0;
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024;
    unit += 1;
  }
  return `${amount.toFixed(amount >= 10 ? 1 : 2)} ${units[unit]}`;
}

function formatDate(value: string) {
  return new Date(value).toLocaleString();
}

function stateLabel(state: DiagnosticComponentState) {
  const labels: Record<DiagnosticComponentState, string> = {
    available: "Готов",
    unavailable: "Недоступен",
    not_configured: "Не настроен",
    not_checked: "Не проверен",
  };
  return labels[state];
}

function backupKindLabel(kind: BackupKind) {
  const labels: Record<BackupKind, string> = {
    daily: "дневная",
    manual: "ручная",
    pre_migration: "перед миграцией",
    emergency: "аварийная",
    unknown: "неизвестная",
  };
  return labels[kind];
}

function metricLabel(metric: DiagnosticStorageMetric) {
  return metric.accessible ? `${formatBytes(metric.bytes)} · ${metric.files} файлов` : "Проверено частично";
}

onMounted(() => {
  void refresh();
});
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Состояние приложения</h1>
      <div class="toolbar">
        <button class="btn" :disabled="loading || exporting" @click="refresh">
          {{ loading ? "Проверяем…" : "Проверить снова" }}
        </button>
        <button class="btn btn-primary" :disabled="loading || exporting || !report" @click="exportArchive">
          {{ exporting ? "Экспорт…" : "Экспорт диагностики" }}
        </button>
      </div>
    </header>

    <div class="page-body diagnostics-body">
      <p class="privacy-note">
        Диагностика остаётся локальной. Архив не содержит документы, аннотации, заметки,
        настройки, URL, реальные пути или секреты.
      </p>

      <div v-if="!report" class="empty card">
        <p>{{ loading ? "Собираем локальные показатели…" : "Диагностика пока недоступна." }}</p>
      </div>

      <template v-else>
        <section class="diagnostic-grid">
          <div class="card metric-card">
            <span class="metric-title">Версия</span>
            <strong>SoheiDesk {{ report.app_version }}</strong>
            <span class="muted">
              DB v{{ report.database_schema_version ?? "?" }} · поддерживается v{{ report.supported_schema_version }}
            </span>
          </div>
          <div class="card metric-card">
            <span class="metric-title">Целостность</span>
            <strong :class="report.integrity.ok ? 'status-ok' : 'status-error'">
              {{ report.integrity.ok ? "Исправна" : "Требует внимания" }}
            </strong>
            <span class="muted">{{ report.integrity.message }}</span>
          </div>
          <div class="card metric-card">
            <span class="metric-title">Последняя успешная копия</span>
            <template v-if="report.last_successful_backup">
              <strong>{{ formatDate(report.last_successful_backup.created_at) }}</strong>
              <span class="muted">
                {{ backupKindLabel(report.last_successful_backup.kind) }} ·
                {{ formatBytes(report.last_successful_backup.size_bytes) }} ·
                DB v{{ report.last_successful_backup.schema_version }}
              </span>
            </template>
            <strong v-else class="status-warn">Копий пока нет</strong>
          </div>
        </section>

        <section>
          <h2>Хранилище</h2>
          <div class="diagnostic-grid storage-grid">
            <div class="card metric-card">
              <span class="metric-title">База данных + WAL</span>
              <strong>{{ metricLabel(report.storage.database) }}</strong>
            </div>
            <div class="card metric-card">
              <span class="metric-title">Вложения</span>
              <strong>{{ metricLabel(report.storage.attachments) }}</strong>
            </div>
            <div class="card metric-card">
              <span class="metric-title">Извлечённые медиа</span>
              <strong>{{ metricLabel(report.storage.media) }}</strong>
            </div>
            <div class="card metric-card">
              <span class="metric-title">Поисковый индекс</span>
              <strong>{{ metricLabel(report.storage.search_index) }}</strong>
            </div>
          </div>
        </section>

        <section>
          <h2>Компоненты</h2>
          <div class="component-list">
            <div
              v-for="(component, name) in report.components"
              :key="name"
              class="list-item component-row"
            >
              <div>
                <strong>{{ name === "pdf_worker" ? "PDF worker" : name === "chroma_tsvet" ? "ChromaTsvet" : name.toUpperCase() }}</strong>
                <div class="muted component-message">{{ component.message }}</div>
              </div>
              <div class="component-state">
                <span class="badge" :class="`state-${component.state}`">{{ stateLabel(component.state) }}</span>
                <span v-if="component.version" class="muted mono">{{ component.version }}</span>
              </div>
            </div>
          </div>
        </section>

        <section>
          <h2>Последние ошибки</h2>
          <div v-if="report.recent_errors.length === 0" class="card muted">
            Зарегистрированных ошибок нет.
          </div>
          <div v-else class="error-list">
            <div v-for="event in report.recent_errors" :key="`${event.timestamp}-${event.category}`" class="list-item">
              <div>
                <strong>{{ event.message }}</strong>
                <div class="muted mono">{{ event.category }}</div>
              </div>
              <span class="muted">{{ formatDate(event.timestamp) }}</span>
            </div>
          </div>
        </section>

        <p v-if="exportMessage" class="export-message" role="status">{{ exportMessage }}</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.diagnostics-body {
  display: flex;
  flex-direction: column;
  gap: 22px;
  max-width: 1100px;
  width: 100%;
}
.privacy-note {
  margin: 0;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--text-muted);
  background: var(--bg-elevated);
  line-height: 1.45;
}
.diagnostic-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}
.storage-grid {
  grid-template-columns: repeat(4, minmax(0, 1fr));
}
.metric-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}
.metric-title {
  color: var(--text-muted);
  font-size: 0.76rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
h2 {
  margin: 0 0 10px;
  font-size: 0.95rem;
}
.component-list,
.error-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.component-row {
  align-items: flex-start;
}
.component-message {
  margin-top: 4px;
  font-size: 0.82rem;
}
.component-state {
  display: flex;
  flex-direction: column;
  gap: 5px;
  align-items: flex-end;
}
.mono {
  font-family: var(--mono);
  font-size: 0.75rem;
}
.status-ok,
.state-available {
  color: var(--success);
}
.status-error,
.state-unavailable {
  color: var(--danger);
}
.status-warn,
.state-not_configured,
.state-not_checked {
  color: var(--text-muted);
}
.export-message {
  margin: 0;
  color: var(--success);
}
@media (max-width: 900px) {
  .diagnostic-grid,
  .storage-grid {
    grid-template-columns: 1fr 1fr;
  }
}
</style>
