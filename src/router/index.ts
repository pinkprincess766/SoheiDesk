import { createRouter, createWebHashHistory } from "vue-router";
import ReaderView from "../views/ReaderView.vue";
import JournalView from "../views/JournalView.vue";
import LibraryView from "../views/LibraryView.vue";
import SettingsView from "../views/SettingsView.vue";
import SearchView from "../views/SearchView.vue";
import IntegrationsView from "../views/IntegrationsView.vue";
import ExportView from "../views/ExportView.vue";
import LiteratureView from "../views/LiteratureView.vue";
import OcrView from "../views/OcrView.vue";
import RssView from "../views/RssView.vue";
import PluginsView from "../views/PluginsView.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/reader" },
    { path: "/reader", name: "reader", component: ReaderView },
    { path: "/library", name: "library", component: LibraryView },
    { path: "/journal", name: "journal", component: JournalView },
    { path: "/export", name: "export", component: ExportView },
    { path: "/literature", name: "literature", component: LiteratureView },
    { path: "/search", name: "search", component: SearchView },
    { path: "/rss", name: "rss", component: RssView },
    { path: "/integrations", name: "integrations", component: IntegrationsView },
    { path: "/ocr", name: "ocr", component: OcrView },
    { path: "/plugins", name: "plugins", component: PluginsView },
    { path: "/settings", name: "settings", component: SettingsView },
  ],
});

export default router;
