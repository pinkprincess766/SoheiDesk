<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import { useAppStore, applyTheme } from "./stores/app";
import { useLibraryStore } from "./stores/library";

const app = useAppStore();
const library = useLibraryStore();
const router = useRouter();

function isMod(e: KeyboardEvent) {
  return e.metaKey || e.ctrlKey;
}

function onKey(e: KeyboardEvent) {
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
    library.openViaDialog();
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
  if (e.key === "?" && !isMod(e)) {
    e.preventDefault();
    router.push("/settings");
  }
  if (e.key === "Escape") {
    app.setError(null);
  }
}

onMounted(async () => {
  applyTheme(app.theme);
  await app.loadInfo();
  window.addEventListener("keydown", onKey);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
});
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <img class="brand-logo" src="/app-icon.png" width="36" height="36" alt="" />
        <div class="brand-text">
          <strong>SoheiDesk</strong>
          <span>научный ридер</span>
        </div>
      </div>
      <nav class="nav">
        <div class="nav-group">Работа</div>
        <router-link to="/reader">Ридер</router-link>
        <router-link to="/library">Библиотека</router-link>
        <router-link to="/journal">Журнал</router-link>
        <router-link to="/search">Поиск</router-link>

        <div class="nav-group">Выход</div>
        <router-link to="/export">Экспорт</router-link>
        <router-link to="/literature">Литература</router-link>

        <div class="nav-group">Сеть</div>
        <router-link to="/rss">RSS</router-link>
        <router-link to="/integrations">Zotero</router-link>

        <div class="nav-group">Сервис</div>
        <router-link to="/ocr">OCR</router-link>
        <router-link to="/plugins">Плагины</router-link>
        <router-link to="/settings">Настройки</router-link>
      </nav>
      <div class="sidebar-footer">
        v{{ app.info?.version || "0.4.0" }}
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
