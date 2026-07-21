<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { renderMarkdown } from "../../utils/markdown";
import type { Annotation, ReflowPosition } from "../../types";
import { useAnnotationsStore } from "../../stores/annotations";

const props = defineProps<{
  text: string;
  mode: "txt" | "md";
  documentId: string;
}>();

const annotations = useAnnotationsStore();
const contentRef = ref<HTMLElement | null>(null);

const displayHtml = computed(() => {
  if (props.mode === "md") return renderMarkdown(props.text);
  // plain text as pre-like paragraphs preserving offsets via data
  return escapeHtml(props.text).replace(/\n/g, "<br/>");
});

function escapeHtml(s: string) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function parsePos(a: Annotation): ReflowPosition | null {
  try {
    return JSON.parse(a.position_json) as ReflowPosition;
  } catch {
    return null;
  }
}

/** Map selection offsets within the original plain text for TXT; for MD use rendered text approx. */
function getSelectionOffsets(): ReflowPosition | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !contentRef.value) return null;
  if (!contentRef.value.contains(sel.anchorNode) || !contentRef.value.contains(sel.focusNode)) {
    return null;
  }

  const range = sel.getRangeAt(0);
  const pre = range.cloneRange();
  pre.selectNodeContents(contentRef.value);
  pre.setEnd(range.startContainer, range.startOffset);
  const start = pre.toString().length;
  const end = start + range.toString().length;
  if (end <= start) return null;
  return {
    start_offset: start,
    end_offset: end,
    quote: range.toString().slice(0, 200),
  };
}

async function onMouseUp() {
  if (annotations.mode === "none") return;
  await nextTick();
  const pos = getSelectionOffsets();
  if (!pos) return;

  let content: string | null = null;
  if (annotations.mode === "comment") {
    content = window.prompt("Комментарий:") || "";
    if (!content.trim()) {
      window.getSelection()?.removeAllRanges();
      return;
    }
  }

  await annotations.create({
    document_id: props.documentId,
    ann_type: annotations.mode,
    page: null,
    position_json: JSON.stringify(pos),
    content,
    color: annotations.activeColor,
  });
  window.getSelection()?.removeAllRanges();
  await nextTick();
  applyHighlights();
}

function applyHighlights() {
  const el = contentRef.value;
  if (!el) return;

  // Reset content
  if (props.mode === "md") {
    el.innerHTML = displayHtml.value;
  } else {
    el.innerHTML = escapeHtml(props.text).replace(/\n/g, "<br/>");
  }

  // For TXT we can map offsets on textContent; for MD offsets are on rendered text
  const full = el.textContent || "";
  const marks = annotations.items
    .map((a) => ({ a, pos: parsePos(a) }))
    .filter((x): x is { a: Annotation; pos: ReflowPosition } => !!x.pos)
    .sort((x, y) => y.pos.start_offset - x.pos.start_offset);

  for (const { a, pos } of marks) {
    if (pos.start_offset < 0 || pos.end_offset > full.length) continue;
    try {
      wrapRange(el, pos.start_offset, pos.end_offset, a);
    } catch {
      /* ignore bad ranges */
    }
  }
}

function wrapRange(root: HTMLElement, start: number, end: number, ann: Annotation) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let pos = 0;
  let startNode: Text | null = null;
  let startOff = 0;
  let endNode: Text | null = null;
  let endOff = 0;

  while (walker.nextNode()) {
    const node = walker.currentNode as Text;
    const len = node.data.length;
    if (!startNode && pos + len >= start) {
      startNode = node;
      startOff = start - pos;
    }
    if (pos + len >= end) {
      endNode = node;
      endOff = end - pos;
      break;
    }
    pos += len;
  }
  if (!startNode || !endNode) return;

  const range = document.createRange();
  range.setStart(startNode, startOff);
  range.setEnd(endNode, endOff);

  const mark = document.createElement("mark");
  mark.style.background = ann.color || "#f7e07c";
  mark.style.opacity = "0.85";
  mark.title = ann.content || ann.ann_type;
  mark.dataset.annId = ann.id;
  try {
    range.surroundContents(mark);
  } catch {
    // partial nodes — extract contents
    const frag = range.extractContents();
    mark.appendChild(frag);
    range.insertNode(mark);
  }
}

watch(
  () => [props.text, props.mode, annotations.items],
  async () => {
    await nextTick();
    applyHighlights();
  },
  { deep: true },
);

onMounted(async () => {
  await annotations.load(props.documentId);
  await nextTick();
  applyHighlights();
});
</script>

<template>
  <div
    ref="contentRef"
    class="reflow-viewer"
    :class="mode === 'txt' ? 'text-viewer' : 'md-viewer'"
    @mouseup="onMouseUp"
  />
</template>

<style scoped>
.reflow-viewer {
  user-select: text;
}
.reflow-viewer :deep(mark) {
  border-radius: 2px;
  padding: 0 1px;
}
</style>
