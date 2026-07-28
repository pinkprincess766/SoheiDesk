<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useLibraryStore } from "../../stores/library";
import { useUiModeStore } from "../../stores/uiMode";
import { useAppStore } from "../../stores/app";
import { useAnnotationsStore } from "../../stores/annotations";
import SimpleReader from "./SimpleReader.vue";

const library = useLibraryStore();
const ui = useUiModeStore();
const app = useAppStore();
const annotations = useAnnotationsStore();

const editOpen = ref(false);
const settingsOpen = ref(false);
const readerRef = ref<InstanceType<typeof SimpleReader> | null>(null);

const bodyText = computed(() => {
  const o = library.current?.opened;
  if (!o) return "";
  // Simple = reflow text only (PDF text comes from Rust backend)
  const t = (o.text || "").trim();
  if (t) return t;
  if (o.doc_type === "pdf") {
    return (
      `# ${o.title}\n\n` +
      `Текст PDF не получен.\n\n` +
      `• Скан без OCR — в Settings переключитесь в Обычный режим (страницы).\n` +
      `• Или закройте и откройте файл снова после обновления приложения.\n`
    );
  }
  return `# (empty)\n\nНет текста для отображения.`;
});

async function openFile() {
  editOpen.value = false;
  settingsOpen.value = false;
  await library.openViaDialog();
}

function closeDoc() {
  library.clearCurrent();
  annotations.clear();
}

async function quitApp() {
  // Prefer Rust exit (reliable). Window close needs ACL permissions.
  try {
    await invoke("quit_app");
    return;
  } catch (e) {
    console.warn("quit_app invoke failed", e);
  }
  try {
    await getCurrentWindow().close();
    return;
  } catch (e) {
    console.warn("window.close failed", e);
  }
  try {
    await getCurrentWindow().destroy();
  } catch {
    app.setError("Не удалось закрыть приложение — закройте окно крестиком (⌘Q).");
  }
}

function toggleEdit() {
  settingsOpen.value = false;
  editOpen.value = !editOpen.value;
}

function toggleSettings() {
  editOpen.value = false;
  settingsOpen.value = !settingsOpen.value;
}

function editHighlight() {
  editOpen.value = false;
  readerRef.value?.startTool("highlight");
}

function editComment() {
  editOpen.value = false;
  readerRef.value?.startTool("comment");
}

function switchNormal() {
  ui.setMode("normal");
}
</script>

<template>
  <div class="simple-shell">
    <header class="simple-bar">
      <button class="sbtn primary" :disabled="library.loading" @click="openFile">
        {{ library.loading ? "…" : "Open" }}
      </button>

      <div class="menu-wrap">
        <button class="sbtn" :disabled="!library.current" @click="toggleEdit">Edit ▾</button>
        <div v-if="editOpen" class="menu">
          <button @click="editHighlight">Highlight · ⌥1</button>
          <button @click="editComment">Comment · ⌥2</button>
          <button @click="readerRef?.cancelTool(); editOpen = false">Cancel · Esc</button>
        </div>
      </div>

      <div class="menu-wrap">
        <button class="sbtn" @click="toggleSettings">Settings ▾</button>
        <div v-if="settingsOpen" class="menu">
          <button :class="{ on: app.theme === 'dark' }" @click="app.setTheme('dark')">Theme · Dark</button>
          <button :class="{ on: app.theme === 'light' }" @click="app.setTheme('light')">Theme · Light</button>
          <button :class="{ on: app.theme === 'system' }" @click="app.setTheme('system')">Theme · Auto</button>
          <hr />
          <button @click="switchNormal">→ Обычный режим</button>
        </div>
      </div>

      <button v-if="library.current" class="sbtn" @click="closeDoc">Close</button>
      <button class="sbtn danger" @click="quitApp">Quit</button>

      <span class="grow" />
      <span class="muted mono">simple</span>
    </header>

    <div v-if="app.error" class="error-banner" role="alert">
      <span style="flex: 1">{{ app.error }}</span>
      <button class="btn" style="padding: 2px 8px; font-size: 0.8rem" @click="app.setError(null)">Esc</button>
    </div>

    <div v-if="library.loading" class="simple-empty">
      <p class="mono">sohei›</p>
      <p class="muted">{{ library.status || "Loading…" }}</p>
      <p class="muted" style="font-size: 0.8rem">PDF: первый раз может занять 5–30 с (текст кэшируется)</p>
    </div>

    <SimpleReader
      v-else-if="library.current"
      ref="readerRef"
      :text="bodyText"
      :document-id="library.current.document.id"
      :title="library.current.opened.title"
    />

    <div v-else class="simple-empty">
      <p class="mono">sohei›</p>
      <p class="muted">Open · Edit · Settings · Quit</p>
      <button class="btn btn-primary" :disabled="library.loading" @click="openFile">Open file…</button>
    </div>

    <!-- click-away -->
    <div
      v-if="editOpen || settingsOpen"
      class="scrim"
      @click="editOpen = false; settingsOpen = false"
    />
  </div>
</template>

<style scoped>
.simple-shell {
  height: 100%;
  min-height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
  position: relative;
  background: var(--bg);
}
/* fill window when Simple is root */
:global(html),
:global(body),
:global(#app) {
  height: 100%;
}
.simple-bar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-elevated);
  z-index: 5;
}
.sbtn {
  font-family: var(--mono);
  font-size: 0.78rem;
  font-weight: 600;
  padding: 7px 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
}
.sbtn.primary {
  background: var(--accent);
  border-color: transparent;
  color: #fff;
}
.sbtn.danger {
  color: var(--danger);
}
.sbtn:disabled {
  opacity: 0.45;
}
.menu-wrap {
  position: relative;
}
.menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  min-width: 200px;
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 6px;
  box-shadow: var(--shadow);
  z-index: 10;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.menu button {
  text-align: left;
  border: none;
  background: transparent;
  color: var(--text);
  padding: 8px 10px;
  border-radius: 6px;
  font-size: 0.85rem;
  font-family: var(--mono);
}
.menu button:hover,
.menu button.on {
  background: var(--accent-soft);
  color: var(--accent);
}
.menu hr {
  border: none;
  border-top: 1px solid var(--border);
  margin: 4px 0;
}
.grow {
  flex: 1;
}
.mono {
  font-family: var(--mono);
  font-size: 0.75rem;
}
.simple-empty {
  flex: 1;
  display: grid;
  place-content: center;
  gap: 12px;
  text-align: center;
}
.simple-empty .mono {
  font-size: 1.2rem;
  color: var(--accent);
}
.scrim {
  position: absolute;
  inset: 0;
  z-index: 4;
}
</style>
