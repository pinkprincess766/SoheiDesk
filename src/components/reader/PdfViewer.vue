<script setup lang="ts">
import {
  computed,
  markRaw,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import * as pdfjs from "pdfjs-dist";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Annotation, PdfPosition, PdfRect } from "../../types";
import { useAnnotationsStore } from "../../stores/annotations";
import { useAppStore } from "../../stores/app";

// Bundled into public/ — works in Tauri webview
pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  "/pdf.worker.min.mjs",
  window.location.href,
).toString();

const props = defineProps<{
  path: string;
  title: string;
  documentId: string;
  binaryBase64?: string | null;
  cachePath?: string | null;
}>();

const annotations = useAnnotationsStore();
const app = useAppStore();

/** Always-mounted pages root — never gated by v-if / v-show */
const containerRef = ref<HTMLElement | null>(null);
const scale = ref(1.15);
const pageCount = ref(0);
const loading = ref(true);
const rendering = ref(false);
const error = ref<string | null>(null);
const status = ref("");

/**
 * pdf.js uses JS private fields (#…). A Vue deep `ref` wraps the object in a Proxy
 * and then getPage/render throw "Cannot read from private field".
 * Keep the document as a plain module-level variable (not reactive).
 */
let pdfDoc: pdfjs.PDFDocumentProxy | null = null;

/** Ignore stale async results when props change mid-load */
let loadGen = 0;
/** How many pages have been fully rendered into the DOM */
const renderedUpTo = ref(0);
/** Batch size for progressive render */
const BATCH = 4;
/** First paint: show something fast */
const FIRST_BATCH = 2;

type PageView = {
  pageNum: number;
  canvas: HTMLCanvasElement;
  overlay: HTMLDivElement;
  viewport: pdfjs.PageViewport;
  scale: number;
};

/** shallowRef so we don't deep-proxy canvas/viewport objects */
const pageViews = shallowRef<PageView[]>([]);
let dragStart: { pageNum: number; x: number; y: number } | null = null;
let dragRect: PdfRect | null = null;
let rubberEl: HTMLDivElement | null = null;
let strokeCss: { x: number; y: number }[] = [];
let strokeSvg: SVGSVGElement | null = null;
let renderAbort = false;

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

function b64ToBytes(b64: string): Uint8Array {
  const raw = atob(b64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

async function loadPdfBytes(): Promise<Uint8Array> {
  if (props.binaryBase64 && props.binaryBase64.length > 100) {
    status.value = "loading embedded PDF…";
    return b64ToBytes(props.binaryBase64);
  }

  const tryPaths = [props.cachePath, props.path].filter(Boolean) as string[];
  for (const p of tryPaths) {
    try {
      status.value = `asset: ${p.slice(0, 40)}…`;
      const url = convertFileSrc(p);
      const res = await fetch(url);
      if (res.ok) {
        const buf = await res.arrayBuffer();
        if (buf.byteLength > 4) {
          const u8 = new Uint8Array(buf);
          if (u8[0] === 0x25 && u8[1] === 0x50 && u8[2] === 0x44 && u8[3] === 0x46) {
            return u8;
          }
        }
      }
    } catch {
      /* next */
    }
  }

  status.value = "reading via backend…";
  const file = await invoke<{ base64: string; size: number }>("read_authorized_file", {
    path: props.cachePath || props.path,
    documentId: props.documentId,
  });
  return b64ToBytes(file.base64);
}

/** Resolve the pages container — always in the template, so this is a safety net only. */
function getContainer(): HTMLElement {
  const el = containerRef.value;
  if (!el) {
    throw new Error(
      "PDF container missing (internal). Перезапустите app:dev и нажмите «Перезагрузить».",
    );
  }
  return el;
}

async function loadPdf() {
  const gen = ++loadGen;
  renderAbort = true; // cancel any in-flight progressive render
  loading.value = true;
  rendering.value = false;
  error.value = null;
  status.value = "starting…";
  pageCount.value = 0;
  renderedUpTo.value = 0;
  destroyPages();

  try {
    const bytes = await loadPdfBytes();
    if (gen !== loadGen) return;
    if (bytes.byteLength < 5) {
      throw new Error("PDF data is empty");
    }
    status.value = `parsing ${Math.round(bytes.byteLength / 1024)} KB…`;

    // Fresh buffer (pdf.js prefers non-shared / transferable-friendly data)
    const copy = Uint8Array.from(bytes);

    const task = pdfjs.getDocument({
      data: copy,
      useSystemFonts: true,
      disableStream: true,
    });
    const doc = await task.promise;
    if (gen !== loadGen) {
      try {
        (doc as { destroy?: () => void }).destroy?.();
      } catch {
        /* */
      }
      return;
    }

    // Drop previous doc
    try {
      const prev = pdfDoc as unknown as { destroy?: () => void; cleanup?: () => void } | null;
      prev?.cleanup?.();
      prev?.destroy?.();
    } catch {
      /* */
    }

    // markRaw: belt-and-suspenders if anything reactive ever touches this
    pdfDoc = markRaw(doc) as unknown as pdfjs.PDFDocumentProxy;
    pageCount.value = doc.numPages;
    status.value = `${doc.numPages} page(s) — preparing view…`;

    // Show the stage immediately (container is always mounted)
    loading.value = false;
    error.value = null;
    await nextTick();

    // One more frame so layout is applied (flex min-height etc.)
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
    if (gen !== loadGen) return;

    getContainer(); // assert early with a clear message

    renderAbort = false;
    rendering.value = true;
    status.value = "rendering…";

    await renderPagesProgressive(gen);
    if (gen !== loadGen) return;

    await annotations.load(props.documentId);
    if (gen !== loadGen) return;

    rendering.value = false;
    status.value = "";
  } catch (e) {
    if (gen !== loadGen) return;
    console.error("PDF load error", e);
    const msg = e instanceof Error ? e.message : String(e);
    error.value = `Не удалось открыть PDF: ${msg}`;
    app.setError(error.value);
    loading.value = false;
    rendering.value = false;
  }
}

function destroyPages() {
  const root = containerRef.value;
  if (root) root.innerHTML = "";
  pageViews.value = [];
  dragStart = null;
  dragRect = null;
  rubberEl = null;
  strokeCss = [];
  strokeSvg = null;
  renderedUpTo.value = 0;
}

function destroyPdfDoc() {
  try {
    const d = pdfDoc as unknown as { destroy?: () => void; cleanup?: () => void } | null;
    d?.cleanup?.();
    d?.destroy?.();
  } catch {
    /* */
  }
  pdfDoc = null;
}

async function renderPage(
  doc: { getPage: (n: number) => Promise<pdfjs.PDFPageProxy> },
  i: number,
): Promise<{ view: PageView; wrap: HTMLDivElement }> {
  const page = await doc.getPage(i);
  const viewport = page.getViewport({ scale: scale.value });
  const wrap = document.createElement("div");
  wrap.className = "pdf-page-wrap";
  wrap.dataset.page = String(i);
  wrap.style.position = "relative";
  wrap.style.width = `${viewport.width}px`;
  wrap.style.margin = "0 auto 16px";
  wrap.style.boxShadow = "0 2px 12px rgba(0,0,0,0.25)";
  wrap.style.background = "#fff";

  const canvas = document.createElement("canvas");
  const outputScale = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = Math.floor(viewport.width * outputScale);
  canvas.height = Math.floor(viewport.height * outputScale);
  canvas.style.width = `${viewport.width}px`;
  canvas.style.height = `${viewport.height}px`;
  canvas.style.display = "block";
  wrap.appendChild(canvas);

  const overlay = document.createElement("div");
  overlay.className = "pdf-overlay";
  overlay.style.position = "absolute";
  overlay.style.left = "0";
  overlay.style.top = "0";
  overlay.style.width = `${viewport.width}px`;
  overlay.style.height = `${viewport.height}px`;
  overlay.style.cursor = annotations.mode === "none" ? "default" : "crosshair";
  wrap.appendChild(overlay);

  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("2d context failed");
  if (outputScale !== 1) {
    ctx.setTransform(outputScale, 0, 0, outputScale, 0, 0);
  }

  await page
    .render({
      canvasContext: ctx,
      viewport,
      canvas,
    } as Parameters<typeof page.render>[0])
    .promise;

  const view: PageView = {
    pageNum: i,
    canvas,
    overlay,
    viewport: markRaw(viewport),
    scale: scale.value,
  };

  overlay.addEventListener("mousedown", (ev) => onOverlayDown(ev, view));
  overlay.addEventListener("mousemove", (ev) => onOverlayMove(ev, view));
  overlay.addEventListener("mouseup", (ev) => onOverlayUp(ev, view));
  paintOverlay(view);
  return { view, wrap };
}

/**
 * Render pages in batches so the first page appears quickly and the UI stays responsive
 * on 600+ page textbooks.
 */
async function renderPagesProgressive(gen: number) {
  const doc = pdfDoc;
  const root = getContainer();
  if (!doc) throw new Error("PDF document not loaded");

  root.innerHTML = "";
  const views: PageView[] = [];
  const total = doc.numPages;
  let i = 1;

  // First batch — paint ASAP
  const firstEnd = Math.min(FIRST_BATCH, total);
  for (; i <= firstEnd; i++) {
    if (renderAbort || gen !== loadGen) return;
    const { view, wrap } = await renderPage(doc, i);
    views.push(view);
    root.appendChild(wrap);
  }
  pageViews.value = views.slice();
  renderedUpTo.value = firstEnd;
  status.value = total > firstEnd ? `стр. ${firstEnd}/${total}…` : "";

  // Yield so Vue paints the first pages
  await new Promise<void>((r) => requestAnimationFrame(() => r()));

  // Remaining pages in batches
  while (i <= total) {
    if (renderAbort || gen !== loadGen) return;
    const batchEnd = Math.min(i + BATCH - 1, total);
    for (; i <= batchEnd; i++) {
      if (renderAbort || gen !== loadGen) return;
      const { view, wrap } = await renderPage(doc, i);
      views.push(view);
      root.appendChild(wrap);
    }
    pageViews.value = views.slice();
    renderedUpTo.value = batchEnd;
    status.value = batchEnd < total ? `стр. ${batchEnd}/${total}` : "";
    // Let the browser breathe between batches
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
  }

  pageViews.value = views;
  renderedUpTo.value = total;
  status.value = "";
}

async function renderAllPages() {
  // Full re-render (e.g. zoom change)
  const gen = loadGen;
  renderAbort = false;
  rendering.value = true;
  try {
    await renderPagesProgressive(gen);
  } finally {
    if (gen === loadGen) rendering.value = false;
  }
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
      svg.appendChild(path);
      continue;
    }

    for (const rect of pos.rects || []) {
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
        const el = document.createElement("div");
        el.style.position = "absolute";
        el.style.left = `${css.left}px`;
        el.style.top = `${css.top}px`;
        el.style.width = `${css.width}px`;
        el.style.height = `${css.height}px`;
        el.style.background = color;
        el.style.opacity = "0.35";
        el.style.pointerEvents = "none";
        el.title = a.content || a.ann_type;
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
  if (annotations.mode === "none" || ev.button !== 0) return;
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
    path.setAttribute(
      "d",
      strokeCss.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" "),
    );
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
    await annotations.create({
      document_id: props.documentId,
      ann_type: "drawing",
      page: view.pageNum,
      position_json: JSON.stringify({ page: view.pageNum, points: pts }),
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
  await annotations.create({
    document_id: props.documentId,
    ann_type: mode,
    page: view.pageNum,
    position_json: JSON.stringify({
      page: view.pageNum,
      rects: [rect],
      shape,
    }),
    content,
    color: annotations.activeColor,
  });
  paintAllOverlays();
}

watch(() => annotations.items, () => paintAllOverlays(), { deep: true });
watch(
  () => [props.path, props.documentId, props.cachePath] as const,
  () => {
    void loadPdf();
  },
);
// binaryBase64 is huge — only re-trigger when presence flips, not on every reactive touch
watch(
  () => (props.binaryBase64 ? props.binaryBase64.length : 0),
  (len, prev) => {
    if (len !== prev) void loadPdf();
  },
);
watch(scale, async () => {
  if (pdfDoc && !loading.value) await renderAllPages();
});

onMounted(() => {
  // Container is in the template from the first paint — ref should be set after mount.
  void loadPdf();
});
onBeforeUnmount(() => {
  loadGen++;
  renderAbort = true;
  destroyPages();
  destroyPdfDoc();
});
</script>

<template>
  <div class="pdf-viewer">
    <div class="pdf-toolbar">
      <button class="btn" :disabled="scale <= 0.5" @click="scale = Math.max(0.5, +(scale - 0.15).toFixed(2))">
        −
      </button>
      <span class="muted">{{ Math.round(scale * 100) }}%</span>
      <button class="btn" :disabled="scale >= 3" @click="scale = Math.min(3, +(scale + 0.15).toFixed(2))">
        +
      </button>
      <span class="muted">
        {{ pageCount ? `${renderedUpTo || 0}/${pageCount} стр.` : "—" }}
      </span>
      <span v-if="rendering && status" class="muted" style="font-size: 0.85rem">{{ status }}</span>
      <button class="btn" :disabled="loading" @click="loadPdf">Перезагрузить</button>
    </div>

    <!--
      Stage always contains the pages container (ref never unmounted).
      Loading / error are overlays — this fixes "PDF container not ready".
    -->
    <div class="pdf-stage">
      <div ref="containerRef" class="pdf-pages" />

      <div v-if="loading" class="pdf-overlay-msg">
        <div>
          <p>Загрузка PDF…</p>
          <p class="muted" style="font-size: 0.85rem">{{ status }}</p>
        </div>
      </div>
      <div v-else-if="error" class="pdf-overlay-msg" style="color: var(--danger)">
        <div style="max-width: 480px">
          <p>{{ error }}</p>
          <button class="btn btn-primary" style="margin-top: 12px" @click="loadPdf">Повторить</button>
          <p class="muted" style="font-size: 0.8rem; margin-top: 12px; word-break: break-all">
            path: {{ path }}
          </p>
        </div>
      </div>
    </div>
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
  flex-wrap: wrap;
}
.pdf-stage {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.pdf-pages {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 16px;
  background: color-mix(in srgb, var(--bg) 80%, #000);
}
.pdf-overlay-msg {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 40px;
  text-align: center;
  background: color-mix(in srgb, var(--bg) 92%, transparent);
  z-index: 2;
}
</style>
