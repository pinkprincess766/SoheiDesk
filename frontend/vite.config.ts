import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

// Resolve from this file so Vite behaves the same when called by pnpm, Tauri, or an IDE.
const frontendRoot = fileURLToPath(new URL(".", import.meta.url));

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  root: frontendRoot,
  plugins: [vue()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("pdfjs-dist")) return "pdfjs";
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Avoid rebuild storms: ignore rust, deps, caches, fixtures
      ignored: [
        "**/src-tauri/**",
        "**/node_modules/**",
        "**/dist/**",
        "**/.git/**",
        "**/fixtures/**",
        "**/*.pdf",
        "**/*.djvu",
      ],
    },
  },
}));
