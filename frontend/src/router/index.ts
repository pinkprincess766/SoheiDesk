import { createRouter, createWebHashHistory } from "vue-router";

/** Lazy routes — faster cold start (don't parse all views up front). */
const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/reader" },
    {
      path: "/reader",
      name: "reader",
      component: () => import("../views/ReaderView.vue"),
    },
    {
      path: "/library",
      name: "library",
      component: () => import("../views/LibraryView.vue"),
    },
    {
      path: "/journal",
      name: "journal",
      component: () => import("../views/JournalView.vue"),
    },
    {
      path: "/export",
      name: "export",
      component: () => import("../views/ExportView.vue"),
    },
    {
      path: "/literature",
      name: "literature",
      component: () => import("../views/LiteratureView.vue"),
    },
    {
      path: "/search",
      name: "search",
      component: () => import("../views/SearchView.vue"),
    },
    {
      path: "/rss",
      name: "rss",
      component: () => import("../views/RssView.vue"),
    },
    {
      path: "/integrations",
      name: "integrations",
      component: () => import("../views/IntegrationsView.vue"),
    },
    {
      path: "/ocr",
      name: "ocr",
      component: () => import("../views/OcrView.vue"),
    },
    {
      path: "/plugins",
      name: "plugins",
      component: () => import("../views/PluginsView.vue"),
    },
    {
      path: "/settings",
      name: "settings",
      component: () => import("../views/SettingsView.vue"),
    },
    {
      path: "/diagnostics",
      name: "diagnostics",
      component: () => import("../views/DiagnosticsView.vue"),
    },
  ],
});

export default router;
