<script setup lang="ts">
import { useUiModeStore } from "../stores/uiMode";

const ui = useUiModeStore();

function pick(mode: "simple" | "normal") {
  ui.setMode(mode);
}
</script>

<template>
  <div class="picker">
    <div class="picker-card">
      <img class="picker-logo" src="/app-icon.png" width="72" height="72" alt="" />
      <h1>SoheiDesk</h1>
      <p class="muted lead">Как работать с документами?</p>

      <button class="mode-btn primary" @click="pick('simple')">
        <span class="mode-title">Простой</span>
        <span class="mode-desc">
          Как терминал: сразу текст, минимум кнопок. Выделение ⌥1 · комментарий ⌥2 · vim-навигация.
        </span>
      </button>

      <button class="mode-btn" @click="pick('normal')">
        <span class="mode-title">Обычный</span>
        <span class="mode-desc">
          Полный ридер: PDF по страницам, библиотека, журнал, поиск, аннотации мышью.
        </span>
      </button>

      <p class="muted foot">Выбор можно сменить в Настройках</p>
    </div>
  </div>
</template>

<style scoped>
.picker {
  min-height: 100%;
  display: grid;
  place-items: center;
  padding: 32px 16px;
  background:
    radial-gradient(800px 400px at 50% 0%, color-mix(in srgb, var(--accent) 12%, transparent), transparent 60%),
    var(--bg);
}
.picker-card {
  width: min(420px, 100%);
  display: flex;
  flex-direction: column;
  gap: 12px;
  align-items: stretch;
}
.picker-logo {
  align-self: center;
  border-radius: 16px;
  aspect-ratio: 1;
  object-fit: cover;
  border: 1px solid var(--border);
  box-shadow: var(--shadow);
}
.picker-card h1 {
  margin: 0;
  text-align: center;
  font-size: 1.5rem;
  letter-spacing: -0.04em;
}
.lead {
  text-align: center;
  margin: 0 0 8px;
  font-size: 0.95rem;
}
.mode-btn {
  text-align: left;
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text);
  border-radius: var(--radius);
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  transition: border-color 0.15s, transform 0.12s;
}
.mode-btn:hover {
  border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
}
.mode-btn:active {
  transform: scale(0.99);
}
.mode-btn.primary {
  border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
  background: color-mix(in srgb, var(--accent) 8%, var(--bg-elevated));
}
.mode-title {
  font-weight: 700;
  font-size: 1.05rem;
  letter-spacing: -0.02em;
}
.mode-desc {
  font-size: 0.85rem;
  color: var(--text-muted);
  line-height: 1.45;
}
.foot {
  text-align: center;
  font-size: 0.75rem;
  margin: 8px 0 0;
}
</style>
