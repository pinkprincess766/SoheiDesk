<script setup lang="ts">
import { onMounted, onUnmounted, watch } from "vue";
import { useRouter } from "vue-router";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore, applyTheme } from "./stores/app";
import { useLibraryStore } from "./stores/library";
import { useUiModeStore } from "./stores/uiMode";
import ModePicker from "./components/ModePicker.vue";
import SimpleShell from "./components/simple/SimpleShell.vue";

const app = useAppStore();
const library = useLibraryStore();
const ui = useUiModeStore();
const router = useRouter();

let unlistenOpen: UnlistenFn | null = null;

function isMod(e: KeyboardEvent) {
  return e.metaKey || e.ctrlKey;
}

function onKey(e: KeyboardEvent) {
  if (ui.needsPicker || ui.isSimple) return; // Simple has its own keys

  const t = e.target as HTMLElement | null;
  const typing =
    t &&
    (t.tagName === "INPUT" ||
      t.tagName === "TEXTAREA" ||
      t.tagName === "SELECT" ||
      t.isContentEditable);
  if (typing && e.key !== "Escape") return;

  if (isMod(e) && e.key.toLowerCase() === "o") {
    e.preventDefault();
    router.push("/reader");
    void library.openViaDialog();
    return;
  }
  if (isMod(e) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    router.push("/search");
    return;
  }
  if (isMod(e) && e.key.toLowerCase() === "j") {
    e.preventDefault();
    router.push("/journal");
    return;
  }
  if (isMod(e) && e.key.toLowerCase() === "e") {
    e.preventDefault();
    router.push("/export");
    return;
  }
  if (isMod(e) && e.key === ",") {
    e.preventDefault();
    router.push("/settings");
    return;
  }
  if (e.key === "Escape") app.setError(null);
}

async function openPathsForReading(paths: string[]) {
  if (!paths.length) return;
  if (ui.isNormal) await router.push("/reader");
  await library.openPath(paths[paths.length - 1]);
}

onMounted(async () => {
  applyTheme(app.theme);
  await app.loadInfo();
  window.addEventListener("keydown", onKey);

  try {
    const pending = await invoke<string[]>("take_pending_open_paths");
    if (pending?.length && !ui.needsPicker) {
      await openPathsForReading(pending);
    }
  } catch {
    /* */
  }

  try {
    unlistenOpen = await listen<string[]>("open-paths", async (ev) => {
      if (ui.needsPicker) return;
      await openPathsForReading(ev.payload || []);
    });
  } catch {
    /* */
  }
});

// When user picks mode after launch with pending files
watch(
  () => ui.mode,
  async (m) => {
    if (!m) return;
    try {
      const pending = await invoke<string[]>("take_pending_open_paths");
      if (pending?.length) await openPathsForReading(pending);
    } catch {
      /* */
    }
  },
);

onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  unlistenOpen?.();
});
</script>

<template>
  <ModePicker v-if="ui.needsPicker" />

  <SimpleShell v-else-if="ui.isSimple" />

  <div v-else class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <img class="brand-logo" src="/app-icon.png" width="36" height="36" alt="" />
        <div class="brand-text">
          <strong>SoheiDesk</strong>
          <span>scientific desk</span>
        </div>
      </div>
      <nav class="nav">
        <div class="nav-group">Work</div>
        <router-link to="/reader">Reader</router-link>
        <router-link to="/library">Library</router-link>
        <router-link to="/journal">Journal</router-link>
        <router-link to="/search">Search</router-link>

        <div class="nav-group">Out</div>
        <router-link to="/export">Export</router-link>
        <router-link to="/literature">Literature</router-link>

        <div class="nav-group">Net</div>
        <router-link to="/rss">RSS</router-link>
        <router-link to="/integrations">Zotero</router-link>

        <div class="nav-group">System</div>
        <router-link to="/ocr">OCR</router-link>
        <router-link to="/plugins">Plugins</router-link>
        <router-link to="/settings">Settings</router-link>
      </nav>
      <div class="sidebar-footer">
        <div class="theme-row">
          <button class="theme-btn" :class="{ on: app.theme === 'dark' }" @click="app.setTheme('dark')">
            Dark
          </button>
          <button class="theme-btn" :class="{ on: app.theme === 'light' }" @click="app.setTheme('light')">
            Light
          </button>
          <button
            class="theme-btn"
            :class="{ on: app.theme === 'system' }"
            @click="app.setTheme('system')"
          >
            Auto
          </button>
        </div>
        <button class="theme-btn" style="width: 100%" @click="ui.setMode('simple')">→ Simple</button>
        <span class="version-pill">v{{ app.info?.version || "1.0.0" }}</span>
      </div>
    </aside>

    <main class="main">
      <div v-if="app.error" class="error-banner" role="alert">
        <span style="flex: 1">{{ app.error }}</span>
        <button class="btn" style="padding: 2px 8px; font-size: 0.8rem" @click="app.setError(null)">
          Esc
        </button>
      </div>
      <router-view />
    </main>
  </div>
</template>
