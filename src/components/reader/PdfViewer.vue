<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as pdfjs from "pdfjs-dist";
import pdfWorker from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { invoke } from "@tauri-apps/api/core";
import type { Annotation, PdfPosition, PdfRect } from "../../types";
import { useAnnotationsStore } from "../../stores/annotations";
import { useAppStore } from "../../stores/app";

pdfjs.GlobalWorkerOptions.workerSrc = pdfWorker;

const props = defineProps<{
  path: string;
  title: string;
  documentId: string;
}>();

const annotations = useAnnotationsStore();
const app = useAppStore();

const containerRef = ref<HTMLElement | null>(null);
const scale = ref(1.2);
const pageCount = ref(0);
const loading = ref(true);
const error = ref<string | null>(null);
const pdfDoc = ref<pdfjs.PDFDocumentProxy | null>(null);

type PageView = {
  pageNum: number; // 1-based
  canvas: HTMLCanvasElement;
  overlay: HTMLDivElement;
  viewport: pdfjs.PageViewport;
  scale: number;
};

const pageViews = ref<PageView[]>([]);
let dragStart: { pageNum: number; x: number; y: number } | null = null;
let dragRect: PdfRect | null = null;
let rubberEl: HTMLDivElement | null = null;
/** freehand path in CSS coords while drawing */
let strokeCss: { x: number; y: number }[] = [];
let strokeSvg: SVGSVGElement | null = null;

const pageAnns = computed(() => {
  const map = new Map<number, Annotation[]>();
  for (const a of annotations.items) {
    if (a.page == null) continue;
    const list = map.get(a.page) || [];
    list.push(a);
    map.set(a.page, list);
  }
  return map;
});

function parsePos(a: Annotation): PdfPosition | null {
  try {
    return JSON.parse(a.position_json) as PdfPosition;
  } catch {
    return null;
  }
}

/** Page-space rect → CSS overlay via pdf.js viewport (handles scale / y-flip). */
function pageRectToCss(rect: PdfRect, viewport: pdfjs.PageViewport) {
  const [x1, y1] = viewport.convertToViewportPoint(rect.x, rect.y);
  const [x2, y2] = viewport.convertToViewportPoint(rect.x + rect.w, rect.y + rect.h);
  const left = Math.min(x1, x2);
  const top = Math.min(y1, y2);
  return {
    left,
    top,
    width: Math.abs(x2 - x1),
    height: Math.abs(y2 - y1),
  };
}

/** CSS overlay rect → page-space (stored independently of zoom). */
function cssToPageRect(
  left: number,
  top: number,
  width: number,
  height: number,
  viewport: pdfjs.PageViewport,
): PdfRect {
  const p1 = viewport.convertToPdfPoint(left, top);
  const p2 = viewport.convertToPdfPoint(left + width, top + height);
  const x = Math.min(p1[0], p2[0]);
  const y = Math.min(p1[1], p2[1]);
  const w = Math.abs(p2[0] - p1[0]);
  const h = Math.abs(p2[1] - p1[1]);
  return { x, y, w, h };
}

async function loadPdf() {
  loading.value = true;
  error.value = null;
  destroyPages();
  try {
    const file = await invoke<{ base64: string; mime: string }>("read_authorized_file", {
      path: props.path,
    });
    const raw = atob(file.base64);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);

    const doc = await pdfjs.getDocument({ data: bytes }).promise;
    pdfDoc.value = doc;
    pageCount.value = doc.numPages;
    await renderAllPages();
    await annotations.load(props.documentId);
  } catch (e) {
    error.value = String(e);
    app.setError(String(e));
  } finally {
    loading.value = false;
  }
}

function destroyPages() {
  if (containerRef.value) containerRef.value.innerHTML = "";
  pageViews.value = [];
  dragStart = null;
  dragRect = null;
  rubberEl = null;
}

async function renderAllPages() {
  const doc = pdfDoc.value;
  const root = containerRef.value;
  if (!doc || !root) return;
  root.innerHTML = "";
  const views: PageView[] = [];

  for (let i = 1; i <= doc.numPages; i++) {
    const page = await doc.getPage(i);
    const viewport = page.getViewport({ scale: scale.value });
    const wrap = document.createElement("div");
    wrap.className = "pdf-page-wrap";
    wrap.dataset.page = String(i);
    wrap.style.position = "relative";
    wrap.style.width = `${viewport.width}px`;
    wrap.style.margin = "0 auto 16px";
    wrap.style.boxShadow = "0 2px 12px rgba(0,0,0,0.25)";

    const canvas = document.createElement("canvas");
    canvas.width = viewport.width;
    canvas.height = viewport.height;
    canvas.style.display = "block";
    wrap.appendChild(canvas);

    const overlay = document.createElement("div");
    overlay.className = "pdf-overlay";
    overlay.style.position = "absolute";
    overlay.style.left = "0";
    overlay.style.top = "0";
    overlay.style.width = `${viewport.width}px`;
    overlay.style.height = `${viewport.height}px`;
    overlay.style.cursor =
      annotations.mode === "none" ? "default" : "crosshair";
    wrap.appendChild(overlay);

    const ctx = canvas.getContext("2d")!;
    await page.render({ canvasContext: ctx, viewport, canvas }).promise;

    const view: PageView = {
      pageNum: i,
      canvas,
      overlay,
      viewport,
      scale: scale.value,
    };
    views.push(view);

    overlay.addEventListener("mousedown", (ev) => onOverlayDown(ev, view));
    overlay.addEventListener("mousemove", (ev) => onOverlayMove(ev, view));
    overlay.addEventListener("mouseup", (ev) => onOverlayUp(ev, view));

    root.appendChild(wrap);
    paintOverlay(view);
  }
  pageViews.value = views;
}

function paintOverlay(view: PageView) {
  const overlay = view.overlay;
  const existingRubber = rubberEl && overlay.contains(rubberEl) ? rubberEl : null;
  const existingStroke = strokeSvg && overlay.contains(strokeSvg) ? strokeSvg : null;
  overlay.innerHTML = "";
  if (existingRubber) overlay.appendChild(existingRubber);
  if (existingStroke) overlay.appendChild(existingStroke);

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("width", String(view.viewport.width));
  svg.setAttribute("height", String(view.viewport.height));
  svg.style.position = "absolute";
  svg.style.left = "0";
  svg.style.top = "0";
  svg.style.pointerEvents = "none";

  const anns = pageAnns.value.get(view.pageNum) || [];
  for (const a of anns) {
    const pos = parsePos(a);
    if (!pos) continue;
    const color = a.color || "#f7e07c";

    if (pos.points && pos.points.length > 1) {
      const d = pos.points
        .map((p, i) => {
          const [vx, vy] = view.viewport.convertToViewportPoint(p.x, p.y);
          return `${i === 0 ? "M" : "L"} ${vx} ${vy}`;
        })
        .join(" ");
      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", d);
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", color);
      path.setAttribute("stroke-width", "2");
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("stroke-linejoin", "round");
      svg.appendChild(path);
      continue;
    }

    const rects = pos.rects || [];
    for (const rect of rects) {
      const css = pageRectToCss(rect, view.viewport);
      if (a.ann_type === "ellipse" || pos.shape === "ellipse") {
        const el = document.createElementNS("http://www.w3.org/2000/svg", "ellipse");
        el.setAttribute("cx", String(css.left + css.width / 2));
        el.setAttribute("cy", String(css.top + css.height / 2));
        el.setAttribute("rx", String(css.width / 2));
        el.setAttribute("ry", String(css.height / 2));
        el.setAttribute("fill", "none");
        el.setAttribute("stroke", color);
        el.setAttribute("stroke-width", "2");
        svg.appendChild(el);
      } else if (a.ann_type === "arrow" || pos.shape === "arrow") {
        const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
        line.setAttribute("x1", String(css.left));
        line.setAttribute("y1", String(css.top + css.height));
        line.setAttribute("x2", String(css.left + css.width));
        line.setAttribute("y2", String(css.top));
        line.setAttribute("stroke", color);
        line.setAttribute("stroke-width", "2");
        line.setAttribute("marker-end", "url(#arrowhead)");
        svg.appendChild(line);
      } else if (a.ann_type === "rect" || pos.shape === "rect") {
        const el = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        el.setAttribute("x", String(css.left));
        el.setAttribute("y", String(css.top));
        el.setAttribute("width", String(css.width));
        el.setAttribute("height", String(css.height));
        el.setAttribute("fill", "none");
        el.setAttribute("stroke", color);
        el.setAttribute("stroke-width", "2");
        svg.appendChild(el);
      } else {
        // highlight / comment
        const el = document.createElement("div");
        el.className = "pdf-ann";
        el.style.position = "absolute";
        el.style.left = `${css.left}px`;
        el.style.top = `${css.top}px`;
        el.style.width = `${css.width}px`;
        el.style.height = `${css.height}px`;
        el.style.background = color;
        el.style.opacity = "0.35";
        el.style.pointerEvents = "none";
        el.title = a.content || a.ann_type;
        if (a.ann_type === "comment" && a.content) {
          const pin = document.createElement("div");
          pin.textContent = "💬";
          pin.style.position = "absolute";
          pin.style.right = "-4px";
          pin.style.top = "-14px";
          pin.style.fontSize = "12px";
          el.appendChild(pin);
        }
        overlay.appendChild(el);
      }
    }
  }
  overlay.appendChild(svg);
}

function paintAllOverlays() {
  for (const v of pageViews.value) paintOverlay(v);
}

function localXY(ev: MouseEvent, overlay: HTMLDivElement) {
  const r = overlay.getBoundingClientRect();
  return { x: ev.clientX - r.left, y: ev.clientY - r.top };
}

function onOverlayDown(ev: MouseEvent, view: PageView) {
  if (annotations.mode === "none") return;
  if (ev.button !== 0) return;
  const { x, y } = localXY(ev, view.overlay);
  dragStart = { pageNum: view.pageNum, x, y };
  dragRect = null;
  strokeCss = [];

  if (annotations.mode === "drawing") {
    strokeCss = [{ x, y }];
    strokeSvg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    strokeSvg.setAttribute("width", String(view.viewport.width));
    strokeSvg.setAttribute("height", String(view.viewport.height));
    strokeSvg.style.position = "absolute";
    strokeSvg.style.left = "0";
    strokeSvg.style.top = "0";
    strokeSvg.style.pointerEvents = "none";
    view.overlay.appendChild(strokeSvg);
    return;
  }

  rubberEl = document.createElement("div");
  rubberEl.style.position = "absolute";
  rubberEl.style.border = "1px dashed var(--accent)";
  rubberEl.style.background = "color-mix(in srgb, var(--accent) 20%, transparent)";
  rubberEl.style.pointerEvents = "none";
  if (annotations.mode === "ellipse") rubberEl.style.borderRadius = "50%";
  view.overlay.appendChild(rubberEl);
}

function onOverlayMove(ev: MouseEvent, view: PageView) {
  if (!dragStart || dragStart.pageNum !== view.pageNum) return;
  const { x, y } = localXY(ev, view.overlay);

  if (annotations.mode === "drawing" && strokeSvg) {
    strokeCss.push({ x, y });
    strokeSvg.innerHTML = "";
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    const d = strokeCss
      .map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`)
      .join(" ");
    path.setAttribute("d", d);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", annotations.activeColor);
    path.setAttribute("stroke-width", "2");
    path.setAttribute("stroke-linecap", "round");
    strokeSvg.appendChild(path);
    return;
  }

  if (!rubberEl) return;
  const left = Math.min(dragStart.x, x);
  const top = Math.min(dragStart.y, y);
  const width = Math.abs(x - dragStart.x);
  const height = Math.abs(y - dragStart.y);
  rubberEl.style.left = `${left}px`;
  rubberEl.style.top = `${top}px`;
  rubberEl.style.width = `${width}px`;
  rubberEl.style.height = `${height}px`;
  dragRect = cssToPageRect(left, top, width, height, view.viewport);
}

async function onOverlayUp(_ev: MouseEvent, view: PageView) {
  if (!dragStart || dragStart.pageNum !== view.pageNum) return;
  const mode = annotations.mode;
  const start = dragStart;
  dragStart = null;

  if (mode === "drawing") {
    const pts = strokeCss.map((p) => {
      const [px, py] = view.viewport.convertToPdfPoint(p.x, p.y);
      return { x: px, y: py };
    });
    if (strokeSvg?.parentElement) strokeSvg.parentElement.removeChild(strokeSvg);
    strokeSvg = null;
    strokeCss = [];
    if (pts.length < 2) return;
    const pos: PdfPosition = { page: view.pageNum, points: pts };
    await annotations.create({
      document_id: props.documentId,
      ann_type: "drawing",
      page: view.pageNum,
      position_json: JSON.stringify(pos),
      content: null,
      color: annotations.activeColor,
    });
    paintAllOverlays();
    return;
  }

  const rect = dragRect;
  if (rubberEl?.parentElement) rubberEl.parentElement.removeChild(rubberEl);
  rubberEl = null;
  dragRect = null;

  if (!rect || rect.w < 2 || rect.h < 2) return;

  let content: string | null = null;
  if (mode === "comment") {
    content = window.prompt("Комментарий:") || "";
    if (!content.trim()) return;
  }

  const shape =
    mode === "rect" || mode === "ellipse" || mode === "arrow" ? mode : undefined;
  const pos: PdfPosition = {
    page: view.pageNum,
    rects: [rect],
    shape: shape as PdfPosition["shape"],
  };
  void start;
  await annotations.create({
    document_id: props.documentId,
    ann_type: mode,
    page: view.pageNum,
    position_json: JSON.stringify(pos),
    content,
    color: annotations.activeColor,
  });
  paintAllOverlays();
}

watch(
  () => annotations.items,
  () => paintAllOverlays(),
  { deep: true },
);

watch(
  () => props.path,
  () => loadPdf(),
);

watch(scale, async () => {
  if (pdfDoc.value) await renderAllPages();
});

onMounted(() => loadPdf());
onBeforeUnmount(() => {
  destroyPages();
  // cleanup loading task / document if available
  try {
    const doc = pdfDoc.value as { destroy?: () => void; cleanup?: () => void } | null;
    doc?.destroy?.();
    doc?.cleanup?.();
  } catch {
    /* ignore */
  }
  pdfDoc.value = null;
});</script>

<template>
  <div class="pdf-viewer">
    <div class="pdf-toolbar">
      <button class="btn" :disabled="scale <= 0.5" @click="scale = Math.max(0.5, +(scale - 0.2).toFixed(2))">
        −
      </button>
      <span class="muted">{{ Math.round(scale * 100) }}%</span>
      <button class="btn" :disabled="scale >= 3" @click="scale = Math.min(3, +(scale + 0.2).toFixed(2))">
        +
      </button>
      <span class="muted">{{ pageCount }} стр.</span>
      <span class="badge">page-space ann</span>
    </div>
    <div v-if="loading" class="pdf-placeholder">Загрузка PDF…</div>
    <div v-else-if="error" class="pdf-placeholder" style="color: var(--danger)">{{ error }}</div>
    <div v-else ref="containerRef" class="pdf-pages" />
  </div>
</template>

<style scoped>
.pdf-viewer {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.pdf-toolbar {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-elevated);
}
.pdf-pages {
  flex: 1;
  overflow: auto;
  padding: 16px;
  background: color-mix(in srgb, var(--bg) 80%, #000);
}
</style>
