<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";

interface LiteratureHit {
  source: string;
  external_id: string;
  title: string;
  authors: string;
  year: string | null;
  journal: string | null;
  doi: string | null;
  url: string | null;
  abstract_text: string | null;
  bibtex: string | null;
}

interface BiblioItem {
  id: string;
  source: string;
  external_id: string | null;
  title: string;
  authors: string | null;
  year: string | null;
  journal: string | null;
  doi: string | null;
  url: string | null;
  bibtex: string | null;
  created_at: string;
}

const app = useAppStore();
const tab = ref<"doi" | "arxiv" | "pubmed" | "library">("doi");
const doi = ref("");
const query = ref("");
const hits = ref<LiteratureHit[]>([]);
const library = ref<BiblioItem[]>([]);
const loading = ref(false);
const style = ref("bibtex");

onMounted(() => refreshLibrary());

async function refreshLibrary() {
  try {
    library.value = await invoke<BiblioItem[]>("list_bibliography");
  } catch (e) {
    app.setError(String(e));
  }
}

async function resolveDoi() {
  loading.value = true;
  app.setError(null);
  try {
    const hit = await invoke<LiteratureHit>("resolve_doi", { doi: doi.value });
    hits.value = [hit];
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function searchArxiv() {
  loading.value = true;
  app.setError(null);
  try {
    hits.value = await invoke<LiteratureHit[]>("search_arxiv", {
      query: query.value,
      maxResults: 12,
    });
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function searchPubmed() {
  loading.value = true;
  app.setError(null);
  try {
    hits.value = await invoke<LiteratureHit[]>("search_pubmed", {
      query: query.value,
      maxResults: 12,
    });
  } catch (e) {
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

async function saveHit(hit: LiteratureHit) {
  try {
    await invoke("save_literature_hit", { hit });
    await refreshLibrary();
  } catch (e) {
    app.setError(String(e));
  }
}

async function removeItem(id: string) {
  await invoke("delete_bibliography_item", { id });
  await refreshLibrary();
}

async function exportBiblio() {
  try {
    const content = await invoke<string>("export_bibliography", { style: style.value });
    const ext = style.value === "bibtex" ? "bib" : style.value === "ris" ? "ris" : "txt";
    const path = await save({
      defaultPath: `bibliography.${ext}`,
      filters: [{ name: style.value, extensions: [ext] }],
    });
    if (!path) return;
    await invoke("export_bibliography_to_file", { style: style.value, path });
    // keep content referenced
    void content;
  } catch (e) {
    app.setError(String(e));
  }
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1>Литература</h1>
      <div class="toolbar">
        <button class="btn" :class="{ 'btn-primary': tab === 'doi' }" @click="tab = 'doi'">DOI</button>
        <button class="btn" :class="{ 'btn-primary': tab === 'arxiv' }" @click="tab = 'arxiv'">arXiv</button>
        <button class="btn" :class="{ 'btn-primary': tab === 'pubmed' }" @click="tab = 'pubmed'">PubMed</button>
        <button class="btn" :class="{ 'btn-primary': tab === 'library' }" @click="tab = 'library'; refreshLibrary()">
          Моя библиография
        </button>
      </div>
    </header>

    <div class="page-body" style="max-width: 900px">
      <!-- DOI -->
      <div v-if="tab === 'doi'" class="card" style="margin-bottom: 16px">
        <div class="toolbar">
          <input v-model="doi" class="input" style="flex: 1" placeholder="10.xxxx/..." />
          <button class="btn btn-primary" :disabled="loading" @click="resolveDoi">
            Resolve Crossref
          </button>
        </div>
      </div>

      <!-- arXiv / PubMed -->
      <div v-if="tab === 'arxiv' || tab === 'pubmed'" class="card" style="margin-bottom: 16px">
        <div class="toolbar">
          <input v-model="query" class="input" style="flex: 1" placeholder="поисковый запрос" @keydown.enter="tab === 'arxiv' ? searchArxiv() : searchPubmed()" />
          <button
            class="btn btn-primary"
            :disabled="loading"
            @click="tab === 'arxiv' ? searchArxiv() : searchPubmed()"
          >
            {{ loading ? "…" : "Найти" }}
          </button>
        </div>
        <p class="muted" style="font-size: 0.85rem; margin: 8px 0 0">
          Нужен интернет. Результаты — метаданные; PDF качается вручную по ссылке.
        </p>
      </div>

      <!-- hits -->
      <div v-if="tab !== 'library'" class="list">
        <div v-for="h in hits" :key="h.source + h.external_id" class="list-item">
          <div style="min-width: 0; flex: 1">
            <strong>{{ h.title }}</strong>
            <div class="muted" style="font-size: 0.85rem">
              {{ h.authors }} · {{ h.year || "—" }} · {{ h.journal || h.source }}
            </div>
            <div v-if="h.doi" class="muted" style="font-size: 0.75rem; font-family: var(--mono)">
              DOI: {{ h.doi }}
            </div>
            <div v-if="h.abstract_text" class="muted" style="font-size: 0.8rem; margin-top: 4px">
              {{ h.abstract_text.slice(0, 220) }}…
            </div>
            <a v-if="h.url" :href="h.url" target="_blank" rel="noreferrer" style="font-size: 0.85rem">
              {{ h.url }}
            </a>
          </div>
          <button class="btn btn-primary" @click="saveHit(h)">В библиографию</button>
        </div>
        <div v-if="hits.length === 0" class="muted">Нет результатов.</div>
      </div>

      <!-- library -->
      <div v-else>
        <div class="toolbar" style="margin-bottom: 12px">
          <select v-model="style" class="input">
            <option value="bibtex">BibTeX</option>
            <option value="ris">RIS</option>
            <option value="apa">APA</option>
            <option value="gost">GOST</option>
          </select>
          <button class="btn btn-primary" @click="exportBiblio">Экспорт…</button>
        </div>
        <div class="list">
          <div v-for="b in library" :key="b.id" class="list-item">
            <div style="min-width: 0">
              <strong>{{ b.title }}</strong>
              <div class="muted" style="font-size: 0.85rem">
                {{ b.authors }} · {{ b.year }} · {{ b.source }}
              </div>
            </div>
            <button class="btn btn-danger" @click="removeItem(b.id)">Удалить</button>
          </div>
          <div v-if="library.length === 0" class="muted">Библиография пуста.</div>
        </div>
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
</style>
