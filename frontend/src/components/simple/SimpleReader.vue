<script setup lang="ts">
/**
 * Terminal-like reflow reader for Simple mode.
 * Editing: ⌥1 highlight · ⌥2 comment · hjkl/arrows · Enter confirm · Esc cancel
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import type { Annotation, ReflowPosition } from "../../types";
import { useAnnotationsStore } from "../../stores/annotations";

const props = defineProps<{
  text: string;
  documentId: string;
  title: string;
}>();

const annotations = useAnnotationsStore();
const rootRef = ref<HTMLElement | null>(null);

/** caret index in plain text */
const cursor = ref(0);
/** visual selection while editing */
const selAnchor = ref<number | null>(null);
const selHead = ref<number | null>(null);
type EditTool = "none" | "highlight" | "comment";
const tool = ref<EditTool>("none");
const status = ref("⌥1 highlight · ⌥2 comment · hjkl · Enter · Esc");
const commentOpen = ref(false);
const commentDraft = ref("");

const plain = computed(() => props.text || "");

const selRange = computed(() => {
  if (selAnchor.value == null || selHead.value == null) return null;
  const a = Math.min(selAnchor.value, selHead.value);
  const b = Math.max(selAnchor.value, selHead.value);
  if (b <= a) return null;
  return { start: a, end: b };
});

function parsePos(a: Annotation): ReflowPosition | null {
  try {
    return JSON.parse(a.position_json) as ReflowPosition;
  } catch {
    return null;
  }
}

/** Build HTML with marks + caret/selection overlays via spans */
const rendered = computed(() => {
  const t = plain.value;
  if (!t) return "<span class='muted'>(empty)</span>";

  type Mark = { start: number; end: number; color: string; title: string; kind: string };
  const marks: Mark[] = [];
  for (const a of annotations.items) {
    if (a.anchor_status === "needs_review") continue;
    const pos = parsePos(a);
    if (!pos) continue;
    marks.push({
      start: pos.start_offset,
      end: pos.end_offset,
      color: a.color || "#f7e07c",
      title: a.content || a.ann_type,
      kind: a.ann_type,
    });
  }
  if (selRange.value) {
    marks.push({
      start: selRange.value.start,
      end: selRange.value.end,
      color: tool.value === "comment" ? "#a0e7e5" : "#f7e07c",
      title: "selection",
      kind: "sel",
    });
  }

  // split points
  const points = new Set<number>([0, t.length, cursor.value]);
  for (const m of marks) {
    points.add(Math.max(0, Math.min(t.length, m.start)));
    points.add(Math.max(0, Math.min(t.length, m.end)));
  }
  const sorted = [...points].sort((a, b) => a - b);

  let html = "";
  for (let i = 0; i < sorted.length - 1; i++) {
    const a = sorted[i];
    const b = sorted[i + 1];
    if (b <= a) continue;
    const chunk = escapeHtml(t.slice(a, b));
    const covering = marks.filter((m) => m.start <= a && m.end >= b);
    const isCaret = cursor.value === a;
    let inner = chunk;
    if (covering.length) {
      const top = covering[covering.length - 1];
      const bg =
        top.kind === "sel"
          ? "color-mix(in srgb, var(--accent) 35%, transparent)"
          : top.color;
      inner = `<mark style="background:${bg}" title="${escapeHtml(top.title)}">${chunk}</mark>`;
    }
    if (isCaret && tool.value !== "none") {
      html += `<span class="caret"></span>`;
    }
    html += inner;
  }
  if (cursor.value >= t.length && tool.value !== "none") {
    html += `<span class="caret"></span>`;
  }
  return html.replace(/\n/g, "<br/>");
});

function escapeHtml(s: string) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function clamp(n: number) {
  return Math.max(0, Math.min(plain.value.length, n));
}

function move(delta: number, visual: boolean) {
  if (visual && selAnchor.value == null) {
    selAnchor.value = cursor.value;
  }
  cursor.value = clamp(cursor.value + delta);
  if (visual || tool.value !== "none") {
    if (selAnchor.value == null) selAnchor.value = cursor.value;
    selHead.value = cursor.value;
  }
  scrollCaretIntoView();
}

function moveLine(dir: -1 | 1, visual: boolean) {
  const t = plain.value;
  const i = cursor.value;
  if (dir < 0) {
    const prevNl = t.lastIndexOf("\n", Math.max(0, i - 1));
    const lineStart = prevNl + 1;
    const prevPrev = t.lastIndexOf("\n", Math.max(0, lineStart - 2));
    const prevStart = prevPrev + 1;
    const col = i - lineStart;
    cursor.value = clamp(prevStart + Math.min(col, lineStart - prevStart - (lineStart > 0 ? 0 : 0)));
    // simpler: jump to start of previous line + same col
    const prevLineEnd = lineStart - 1;
    const prevLineStart = t.lastIndexOf("\n", Math.max(0, prevLineEnd - 1)) + 1;
    const prevLen = Math.max(0, prevLineEnd - prevLineStart);
    cursor.value = clamp(prevLineStart + Math.min(col, prevLen));
  } else {
    const nextNl = t.indexOf("\n", i);
    const lineStart = t.lastIndexOf("\n", Math.max(0, i - 1)) + 1;
    const col = i - lineStart;
    if (nextNl < 0) {
      cursor.value = t.length;
    } else {
      const nextStart = nextNl + 1;
      const nextEnd = t.indexOf("\n", nextStart);
      const nextLen = (nextEnd < 0 ? t.length : nextEnd) - nextStart;
      cursor.value = clamp(nextStart + Math.min(col, nextLen));
    }
  }
  if (visual || tool.value !== "none") {
    if (selAnchor.value == null) selAnchor.value = i;
    selHead.value = cursor.value;
  }
  scrollCaretIntoView();
}

function startTool(t: EditTool) {
  tool.value = t;
  selAnchor.value = cursor.value;
  selHead.value = cursor.value;
  status.value =
    t === "highlight"
      ? "HIGHLIGHT · hjkl/arrows expand · Enter save · Esc cancel"
      : t === "comment"
        ? "COMMENT · select range · Enter → type note · Esc cancel"
        : status.value;
}

function cancelTool() {
  tool.value = "none";
  selAnchor.value = null;
  selHead.value = null;
  status.value = "⌥1 highlight · ⌥2 comment · hjkl · Enter · Esc";
}

async function confirmTool() {
  const r = selRange.value;
  if (!r || tool.value === "none") {
    cancelTool();
    return;
  }
  const quote = plain.value.slice(r.start, r.end).slice(0, 200);
  const selectedText = plain.value.slice(r.start, r.end);
  if (tool.value === "comment") {
    commentDraft.value = "";
    commentOpen.value = true;
    return;
  }
  await annotations.create({
    document_id: props.documentId,
    ann_type: "highlight",
    page: null,
    position_json: JSON.stringify({
      start_offset: r.start,
      end_offset: r.end,
      quote,
    } satisfies ReflowPosition),
    content: quote || "выделение",
    color: annotations.activeColor,
    selected_text: selectedText,
    context_before: plain.value.slice(Math.max(0, r.start - 120), r.start),
    context_after: plain.value.slice(r.end, r.end + 120),
  });
  cancelTool();
}

async function submitComment() {
  const r = selRange.value;
  if (!r) {
    commentOpen.value = false;
    cancelTool();
    return;
  }
  const note = commentDraft.value.trim();
  if (!note) {
    commentOpen.value = false;
    return;
  }
  const quote = plain.value.slice(r.start, r.end).slice(0, 200);
  const selectedText = plain.value.slice(r.start, r.end);
  await annotations.create({
    document_id: props.documentId,
    ann_type: "comment",
    page: null,
    position_json: JSON.stringify({
      start_offset: r.start,
      end_offset: r.end,
      quote,
    } satisfies ReflowPosition),
    content: note,
    color: annotations.activeColor,
    selected_text: selectedText,
    context_before: plain.value.slice(Math.max(0, r.start - 120), r.start),
    context_after: plain.value.slice(r.end, r.end + 120),
  });
  commentOpen.value = false;
  cancelTool();
}

function scrollCaretIntoView() {
  nextTick(() => {
    const el = rootRef.value?.querySelector(".caret");
    el?.scrollIntoView({ block: "nearest" });
  });
}

function onKey(e: KeyboardEvent) {
  // don't steal keys from comment modal inputs
  if (commentOpen.value) {
    if (e.key === "Escape") {
      e.preventDefault();
      commentOpen.value = false;
    }
    return;
  }

  const meta = e.metaKey || e.ctrlKey;
  if (meta) return;

  // Option/Alt + 1/2
  if (e.altKey && (e.code === "Digit1" || e.key === "1" || e.key === "¡")) {
    e.preventDefault();
    startTool("highlight");
    return;
  }
  if (e.altKey && (e.code === "Digit2" || e.key === "2" || e.key === "™")) {
    e.preventDefault();
    startTool("comment");
    return;
  }

  if (e.key === "Escape") {
    e.preventDefault();
    cancelTool();
    return;
  }
  if (e.key === "Enter" && tool.value !== "none") {
    e.preventDefault();
    void confirmTool();
    return;
  }

  const vis = tool.value !== "none" || e.shiftKey;
  if (e.key === "h" || e.key === "ArrowLeft") {
    e.preventDefault();
    move(-1, vis);
  } else if (e.key === "l" || e.key === "ArrowRight") {
    e.preventDefault();
    move(1, vis);
  } else if (e.key === "j" || e.key === "ArrowDown") {
    e.preventDefault();
    moveLine(1, vis);
  } else if (e.key === "k" || e.key === "ArrowUp") {
    e.preventDefault();
    moveLine(-1, vis);
  } else if (e.key === "0" || e.key === "Home") {
    e.preventDefault();
    const t = plain.value;
    const lineStart = t.lastIndexOf("\n", Math.max(0, cursor.value - 1)) + 1;
    cursor.value = lineStart;
    if (vis) {
      if (selAnchor.value == null) selAnchor.value = cursor.value;
      selHead.value = cursor.value;
    }
  } else if (e.key === "$" || e.key === "End") {
    e.preventDefault();
    const t = plain.value;
    const next = t.indexOf("\n", cursor.value);
    cursor.value = next < 0 ? t.length : next;
    if (vis) {
      if (selAnchor.value == null) selAnchor.value = cursor.value;
      selHead.value = cursor.value;
    }
  } else if (e.key === "g") {
    e.preventDefault();
    cursor.value = 0;
    if (vis) {
      selAnchor.value = selAnchor.value ?? 0;
      selHead.value = 0;
    }
  } else if (e.key === "G") {
    e.preventDefault();
    cursor.value = plain.value.length;
    if (vis) {
      selAnchor.value = selAnchor.value ?? cursor.value;
      selHead.value = cursor.value;
    }
  }
}

watch(
  () => props.documentId,
  (id) => {
    if (id) void annotations.load(id);
  },
  { immediate: true },
);

onMounted(() => {
  window.addEventListener("keydown", onKey);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
});

defineExpose({ startTool, cancelTool });
</script>

<template>
  <div class="simple-reader">
    <div class="simple-status">
      <span class="title">{{ title }}</span>
      <span class="muted">{{ status }}</span>
      <span v-if="tool !== 'none'" class="badge">{{ tool }}</span>
    </div>
    <div ref="rootRef" class="simple-body" v-html="rendered" />

    <div v-if="commentOpen" class="comment-modal">
      <div class="comment-card">
        <h3>Комментарий</h3>
        <textarea v-model="commentDraft" rows="4" autofocus placeholder="Текст…" />
        <div class="toolbar" style="justify-content: flex-end; gap: 8px; margin-top: 10px">
          <button class="btn" @click="commentOpen = false">Esc</button>
          <button class="btn btn-primary" @click="submitComment">Enter · Save</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.simple-reader {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg);
}
.simple-status {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 16px;
  align-items: center;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  font-family: var(--mono);
  font-size: 0.75rem;
  background: var(--bg-elevated);
}
.simple-status .title {
  font-weight: 600;
  color: var(--text);
  max-width: 40%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.simple-body {
  flex: 1;
  overflow: auto;
  padding: 28px max(24px, calc(50% - 36ch));
  font-family: var(--mono);
  font-size: 0.9rem;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: none;
}
.simple-body :deep(mark) {
  border-radius: 2px;
  padding: 0 1px;
}
.simple-body :deep(.caret) {
  display: inline-block;
  width: 2px;
  height: 1.1em;
  background: var(--accent);
  vertical-align: text-bottom;
  margin: 0 1px;
  animation: blink 1s step-end infinite;
}
@keyframes blink {
  50% {
    opacity: 0;
  }
}
.comment-modal {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.5);
  z-index: 30;
  padding: 20px;
}
.comment-card {
  width: min(400px, 100%);
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 16px;
}
.comment-card h3 {
  margin: 0 0 10px;
}
.comment-card textarea {
  width: 100%;
}
</style>
