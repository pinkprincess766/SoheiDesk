<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../stores/app";

interface RssFeed {
  id: string;
  title: string;
  url: string;
  category: string | null;
  last_fetched_at: string | null;
}

interface RssItem {
  id: string;
  feed_id: string;
  title: string;
  link: string | null;
  summary: string | null;
  published_at: string | null;
  is_read: boolean;
}

const app = useAppStore();
const feeds = ref<RssFeed[]>([]);
const items = ref<RssItem[]>([]);
const selectedFeed = ref<string | null>(null);
const newTitle = ref("");
const newUrl = ref("");
const loading = ref(false);

async function refresh() {
  feeds.value = await invoke<RssFeed[]>("rss_list_feeds");
  items.value = await invoke<RssItem[]>("rss_list_items", {
    feedId: selectedFeed.value,
    limit: 100,
  });
}

onMounted(async () => {
  try {
    await refresh();
  } catch (e) {
    app.setError(String(e));
  }
});

async function addFeed() {
  if (!newUrl.value.trim()) return;
  loading.value = true;
  try {
    await invoke("rss_add_feed", {
      title: newTitle.value || newUrl.value,
      url: newUrl.value.trim(),
      category: null,
    });
    newTitle.value = "";
    newUrl.value = "";
    await refresh();
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function removeFeed(id: string) {
  await invoke("rss_delete_feed", { id });
  if (selectedFeed.value === id) selectedFeed.value = null;
  await refresh();
}

async function fetchOne(id: string) {
  loading.value = true;
  try {
    const n = await invoke<number>("rss_fetch_feed", { id });
    await refresh();
    app.setError(null);
    if (n === 0) {
      /* ok */
    }
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function fetchAll() {
  loading.value = true;
  try {
    await invoke("rss_fetch_all");
    await refresh();
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function selectFeed(id: string | null) {
  selectedFeed.value = id;
  await refresh();
}

async function markRead(id: string) {
  await invoke("rss_mark_read", { id, isRead: true });
  await refresh();
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>RSS журналов</h1>
      <div class="toolbar">
        <button class="btn" :disabled="loading" @click="fetchAll">Обновить все</button>
      </div>
    </header>
    <div class="page-body rss-layout">
      <div class="card side">
        <h3 style="margin: 0 0 10px">Ленты</h3>
        <button
          class="btn"
          style="width: 100%; margin-bottom: 8px"
          :class="{ 'btn-primary': !selectedFeed }"
          @click="selectFeed(null)"
        >
          Все элементы
        </button>
        <div v-for="f in feeds" :key="f.id" class="feed-row">
          <button
            class="btn"
            style="flex: 1; text-align: left"
            :class="{ 'btn-primary': selectedFeed === f.id }"
            @click="selectFeed(f.id)"
          >
            {{ f.title }}
          </button>
          <button class="btn" title="Fetch" @click="fetchOne(f.id)">↻</button>
          <button class="btn btn-danger" @click="removeFeed(f.id)">×</button>
        </div>
        <div class="muted" style="font-size: 0.8rem; margin: 12px 0 6px">Добавить ленту</div>
        <input v-model="newTitle" class="input" placeholder="Название (опц.)" />
        <input v-model="newUrl" class="input" placeholder="https://…/feed.xml" style="margin-top: 6px" />
        <button class="btn btn-primary" style="width: 100%; margin-top: 8px" :disabled="loading" @click="addFeed">
          Добавить
        </button>
        <p class="muted" style="font-size: 0.75rem; margin-top: 10px">
          Примеры: Nature RSS, PLOS, arXiv API RSS, сайты журналов с /rss или /atom.xml
        </p>
      </div>
      <div class="list">
        <div v-if="items.length === 0" class="muted">Нет элементов — добавьте ленту и нажмите ↻</div>
        <div
          v-for="it in items"
          :key="it.id"
          class="list-item"
          :style="{ opacity: it.is_read ? 0.65 : 1 }"
        >
          <div style="min-width: 0; flex: 1">
            <strong>{{ it.title }}</strong>
            <div class="muted" style="font-size: 0.8rem">{{ it.published_at || "—" }}</div>
            <div v-if="it.summary" class="muted" style="font-size: 0.85rem; margin-top: 4px">
              {{ it.summary.slice(0, 200) }}{{ it.summary.length > 200 ? "…" : "" }}
            </div>
            <a v-if="it.link" :href="it.link" target="_blank" rel="noreferrer" style="font-size: 0.85rem">
              Открыть
            </a>
          </div>
          <button v-if="!it.is_read" class="btn" @click="markRead(it.id)">Прочитано</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.rss-layout {
  display: grid;
  grid-template-columns: 280px 1fr;
  gap: 16px;
  max-width: 1100px;
}
@media (max-width: 800px) {
  .rss-layout {
    grid-template-columns: 1fr;
  }
}
.side {
  height: fit-content;
}
.feed-row {
  display: flex;
  gap: 4px;
  margin-bottom: 6px;
}
.input {
  width: 100%;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
}
</style>
