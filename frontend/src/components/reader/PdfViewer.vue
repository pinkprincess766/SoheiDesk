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

const containerRef = ref<HTMLElement | null>(null);
const scale = ref(1.15);
const pageCount = ref(0);
const currentPage = ref(1);
const loading = ref(true);
const rendering = ref(false);
const error = ref<string | null>(null);
const status = ref("");

/** Comment modal (window.prompt is blocked in Tauri webview) */
const commentOpen = ref(false);
const commentText = ref("");
let commentResolve: ((v: string | null) => void) | null = null;

let pdfDoc: pdfjs.PDFDocumentProxy | null = null;
let loadGen = 0;
const renderedUpTo = ref(0);
/** How many pages around viewport to keep rendered (virtualization) */
const PAGE_BUFFER = 2;
/** Placeholder height until measured (CSS px at scale 1.15 ~ A4-ish) */
const EST_PAGE_HEIGHT = 900;

type PageView = {
  pageNum: number;
  canvas: HTMLCanvasElement;
  overlay: HTMLDivElement;
  wrap: HTMLDivElement;
  viewport: pdfjs.PageViewport;
  scale: number;
};

const pageViews = shallowRef<PageView[]>([]);
let dragStart: { pageNum: number; x: number; y: number; view: PageView } | null = null;
let dragRect: PdfRect | null = null;
let rubberEl: HTMLDivElement | null = null;
let strokeCss: { x: number; y: number }[] = [];
let strokeSvg: SVGSVGElement | null = null;
let renderAbort = false;
let drawingActive = false;
let scaleTimer: ReturnType<typeof setTimeout> | null = null;
/** Separate generation for zoom so we can abort mid-render cleanly */
let scaleGen = 0;
/** Scroll ratio 0..1 while re-rendering zoom (more stable than page anchors) */
let scrollRatio = 0;
const ZOOM_TRANSITION_MS = 160;
const ZOOM_RENDER_DEBOUNCE_MS = 220;

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
  return {
    left: Math.min(x1, x2),
    top: Math.min(y1, y2),
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
  return {
    x: Math.min(p1[0], p2[0]),
    y: Math.min(p1[1], p2[1]),
    w: Math.abs(p2[0] - p1[0]),
    h: Math.abs(p2[1] - p1[1]),
  };
}

function b64ToBytes(b64: string): Uint8Array {
  const raw = atob(b64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

async function loadPdfBytes(): Promise<Uint8Array> {
  const tryPaths = [props.cachePath, props.path].filter(Boolean) as string[];
  for (const p of tryPaths) {
    try {
      status.value = `asset: ${p.slice(0, 48)}…`;
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

  let lastErr: unknown = null;
  for (const p of tryPaths) {
    try {
      status.value = `backend: ${p.slice(0, 48)}…`;
      const file = await invoke<{ base64: string; size: number }>("read_authorized_file", {
        path: p,
        documentId: props.documentId,
      });
      if (file.base64 && file.base64.length > 100) return b64ToBytes(file.base64);
    } catch (e) {
      lastErr = e;
    }
  }

  if (props.binaryBase64 && props.binaryBase64.length > 100) {
    status.value = "loading embedded PDF…";
    return b64ToBytes(props.binaryBase64);
  }

  throw new Error(
    lastErr ? `не удалось прочитать PDF (${String(lastErr)})` : "не удалось прочитать PDF",
  );
}

function getContainer(): HTMLElement {
  const el = containerRef.value;
  if (!el) throw new Error("PDF container missing — reload the document.");
  return el;
}

async function loadPdf() {
  renderAbort = true;
  const gen = ++loadGen;
  scaleGen = gen;
  loading.value = true;
  rendering.value = false;
  error.value = null;
  status.value = "starting…";
  pageCount.value = 0;
  currentPage.value = 1;
  renderedUpTo.value = 0;
  cleanupDrag(true);
  destroyPages();

  try {
    const bytes = await loadPdfBytes();
    if (gen !== loadGen) return;
    if (bytes.byteLength < 5) throw new Error("PDF data is empty");
    status.value = `parsing ${Math.round(bytes.byteLength / 1024)} KB…`;

    const copy = Uint8Array.from(bytes);
    const ab = copy.buffer.slice(copy.byteOffset, copy.byteOffset + copy.byteLength);
    const task = pdfjs.getDocument({
      data: new Uint8Array(ab),
      useSystemFonts: true,
      disableStream: true,
      disableRange: true,
      disableAutoFetch: true,
      stopAtErrors: false,
    });
    let doc;
    try {
      doc = await task.promise;
    } catch (pe) {
      const msg = pe instanceof Error ? pe.message : String(pe);
      if (/password|encrypted/i.test(msg)) {
        throw new Error("PDF защищён паролем — SoheiDesk пока не открывает encrypted PDF");
      }
      throw new Error(`pdf.js: ${msg}`);
    }
    if (gen !== loadGen) {
      try {
        (doc as { destroy?: () => void }).destroy?.();
      } catch {
        /* */
      }
      return;
    }

    destroyPdfDoc();
    pdfDoc = markRaw(doc) as unknown as pdfjs.PDFDocumentProxy;
    pageCount.value = doc.numPages;
    status.value = `${doc.numPages} page(s)`;

    loading.value = false;
    error.value = null;
    await nextTick();
    await new Promise<void>((r) => requestAnimationFrame(() => r()));
    if (gen !== loadGen) return;

    getContainer();
    renderAbort = false;
    rendering.value = true;
    status.value = "rendering…";
    await renderPagesProgressive(gen, "load");
    if (gen !== loadGen) return;

    await annotations.load(props.documentId);
    if (gen !== loadGen) return;

    rendering.value = false;
    status.value = "";
    const root = containerRef.value;
    if (root) {
      root.style.overflow = "auto";
      root.style.pointerEvents = "auto";
      root.style.minHeight = "";
    }
    // ensure scroll listener is attached after container is live
    root?.removeEventListener("scroll", onScroll);
    root?.addEventListener("scroll", onScroll, { passive: true });
    updateCurrentPageFromScroll();
  } catch (e) {
    if (gen !== loadGen) return;
    console.error("PDF load error", e);
    error.value = `Не удалось открыть PDF: ${e instanceof Error ? e.message : String(e)}`;
    app.setError(error.value);
    loading.value = false;
    rendering.value = false;
  }
}

function destroyPages() {
  const root = containerRef.value;
  if (root) root.innerHTML = "";
  pageViews.value = [];
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

function captureScrollRatio() {
  const root = containerRef.value;
  if (!root) {
    scrollRatio = 0;
    return;
  }
  const max = Math.max(1, root.scrollHeight - root.clientHeight);
  scrollRatio = Math.min(1, Math.max(0, root.scrollTop / max));
}

function restoreScrollRatio() {
  const root = containerRef.value;
  if (!root) return;
  // Wait a frame so layout has new page heights
  requestAnimationFrame(() => {
    const max = Math.max(0, root.scrollHeight - root.clientHeight);
    root.scrollTop = scrollRatio * max;
    // Ensure overflow scroll still works after DOM rebuild
    root.style.overflow = "auto";
    root.style.pointerEvents = "auto";
    updateCurrentPageFromScroll();
  });
}

/**
 * Give zoom controls immediate feedback using the already-rendered canvases.
 * PDF.js replaces this GPU-scaled preview with crisp canvases after the user
 * pauses, so repeated clicks do not trigger repeated expensive renders.
 */
function applyZoomPreview() {
  const root = containerRef.value;
  if (!root || pageSlots.length === 0) return;

  captureScrollRatio();
  const animate = !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const transition = animate
    ? `transform ${ZOOM_TRANSITION_MS}ms cubic-bezier(0.2, 0.8, 0.2, 1)`
    : "none";
  const estimatedHeight = Math.round(EST_PAGE_HEIGHT * (scale.value / 1.15));

  for (const slot of pageSlots) {
    const view = slot.view;
    if (!view) {
      slot.height = estimatedHeight;
      slot.el.style.minHeight = `${estimatedHeight}px`;
      continue;
    }

    const ratio = scale.value / view.scale;
    const previewWidth = view.viewport.width * ratio;
    const previewHeight = view.viewport.height * ratio;

    view.wrap.style.transformOrigin = "top center";
    view.wrap.style.transition = transition;
    view.wrap.style.willChange = "transform";
    view.wrap.style.transform = `scale(${ratio})`;
    // The transform itself does not participate in layout. Keep slots at the
    // visual size to prevent page overlap and large gaps during the preview.
    slot.el.style.width = `${previewWidth}px`;
    slot.el.style.minHeight = `${previewHeight}px`;
    slot.height = previewHeight;
    // Annotation coordinates belong to the crisp viewport; avoid accepting a
    // stroke during the brief scaled-preview phase.
    view.overlay.style.pointerEvents = "none";
  }

  requestAnimationFrame(() => {
    const max = Math.max(0, root.scrollHeight - root.clientHeight);
    root.scrollTop = scrollRatio * max;
    updateCurrentPageFromScroll();
  });
}

function updateCurrentPageFromScroll() {
  const root = containerRef.value;
  if (!root || pageSlots.length === 0) return;
  const mid = root.scrollTop + 40;
  let page = 1;
  let y = 0;
  for (const s of pageSlots) {
    const h = s.el.offsetHeight || s.height;
    if (y + h > mid) {
      page = s.pageNum;
      break;
    }
    page = s.pageNum;
    y += h + 16;
  }
  currentPage.value = page;
}

async function renderPage(
  doc: { getPage: (n: number) => Promise<pdfjs.PDFPageProxy> },
  i: number,
): Promise<PageView> {
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
  if (outputScale !== 1) ctx.setTransform(outputScale, 0, 0, outputScale, 0, 0);

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
    wrap,
    viewport: markRaw(viewport),
    scale: scale.value,
  };

  overlay.addEventListener("mousedown", (ev) => onOverlayDown(ev, view));
  paintOverlay(view);
  return view;
}

/** Slot per page: placeholder or live canvas */
type PageSlot = {
  pageNum: number;
  el: HTMLDivElement;
  view: PageView | null;
  height: number;
};

let pageSlots: PageSlot[] = [];
let syncingViewport = false;

function isRenderCurrent(gen: number) {
  return !renderAbort && gen === loadGen && gen === scaleGen;
}

/** Build empty page shells (cheap) — only nearby pages get canvas. */
async function setupVirtualPages(gen: number) {
  const doc = pdfDoc;
  const root = getContainer();
  if (!doc) throw new Error("PDF document not loaded");

  root.innerHTML = "";
  root.style.overflow = "auto";
  root.style.pointerEvents = "auto";
  pageSlots = [];
  pageViews.value = [];

  const total = doc.numPages;
  const est = Math.round(EST_PAGE_HEIGHT * (scale.value / 1.15));

  for (let i = 1; i <= total; i++) {
    if (!isRenderCurrent(gen)) return;
    const el = document.createElement("div");
    el.className = "pdf-page-slot";
    el.dataset.page = String(i);
    el.style.minHeight = `${est}px`;
    el.style.margin = "0 auto 16px";
    el.style.width = "fit-content";
    el.style.maxWidth = "100%";
    root.appendChild(el);
    pageSlots.push({ pageNum: i, el, view: null, height: est });
  }
  renderedUpTo.value = total;
  pageCount.value = total;

  root.removeEventListener("scroll", onVirtualScroll);
  root.addEventListener("scroll", onVirtualScroll, { passive: true });
  await syncVisiblePages(gen);
}

function onVirtualScroll() {
  updateCurrentPageFromScroll();
  if (syncingViewport || !pdfDoc) return;
  // debounce mount work
  window.requestAnimationFrame(() => {
    void syncVisiblePages(loadGen);
  });
}

function visibleRange(root: HTMLElement): [number, number] {
  const top = root.scrollTop;
  const bottom = top + root.clientHeight;
  let start = 1;
  let end = pageSlots.length || 1;
  let y = 0;
  for (const s of pageSlots) {
    const h = s.el.offsetHeight || s.height;
    const slotBottom = y + h + 16;
    if (slotBottom >= top - 100 && start === 1 && y <= top) {
      start = s.pageNum;
    }
    if (y <= bottom + 100) end = s.pageNum;
    y = slotBottom;
  }
  start = Math.max(1, start - PAGE_BUFFER);
  end = Math.min(pageSlots.length, end + PAGE_BUFFER);
  return [start, end];
}

async function syncVisiblePages(gen: number) {
  const doc = pdfDoc;
  const root = containerRef.value;
  if (!doc || !root || !isRenderCurrent(gen)) return;
  if (syncingViewport) return;
  syncingViewport = true;
  try {
    const [start, end] = visibleRange(root);
    // Unmount far pages (free canvas memory)
    for (const s of pageSlots) {
      if (!isRenderCurrent(gen)) return;
      if (s.view && (s.pageNum < start || s.pageNum > end)) {
        s.el.innerHTML = "";
        s.height = s.el.offsetHeight || s.height;
        s.el.style.minHeight = `${s.height}px`;
        s.view = null;
      }
    }
    // Mount visible
    const views: PageView[] = [];
    for (const s of pageSlots) {
      if (!isRenderCurrent(gen)) return;
      if (s.pageNum < start || s.pageNum > end) continue;
      if (!s.view) {
        const view = await renderPage(doc, s.pageNum);
        if (!isRenderCurrent(gen)) return;
        s.el.innerHTML = "";
        s.el.appendChild(view.wrap);
        s.el.style.minHeight = "";
        s.view = view;
        s.height = view.wrap.offsetHeight || s.height;
      }
      if (s.view) views.push(s.view);
    }
    pageViews.value = views;
    renderedUpTo.value = end;
  } finally {
    syncingViewport = false;
  }
}

async function renderPagesProgressive(gen: number, _kind: "load" | "scale" = "load") {
  scaleGen = gen;
  await setupVirtualPages(gen);
  status.value = "";
}

async function renderAllPagesPreservingScroll() {
  if (!pdfDoc || loading.value) return;

  renderAbort = true;
  const gen = ++scaleGen;
  loadGen = gen;
  renderAbort = false;

  captureScrollRatio();
  rendering.value = true;
  status.value = "zoom…";
  try {
    await setupVirtualPages(gen);
    if (gen === scaleGen) restoreScrollRatio();
  } finally {
    if (gen === scaleGen) {
      rendering.value = false;
      status.value = "";
      const root = containerRef.value;
      if (root) {
        root.style.overflow = "auto";
        root.style.pointerEvents = "auto";
      }
    }
  }
}

function paintOverlay(view: PageView) {
  if (drawingActive && dragStart?.pageNum === view.pageNum) {
    // Don't wipe in-progress rubber/stroke
    return;
  }
  const overlay = view.overlay;
  overlay.innerHTML = "";

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
      path.setAttribute("stroke-width", "2.5");
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("stroke-linejoin", "round");
      svg.appendChild(path);
      continue;
    }

    for (const rect of pos.rects || []) {
      const css = pageRectToCss(rect, view.viewport);
      if (css.width < 0.5 || css.height < 0.5) continue;

      if (a.ann_type === "ellipse" || pos.shape === "ellipse") {
        const el = document.createElementNS("http://www.w3.org/2000/svg", "ellipse");
        el.setAttribute("cx", String(css.left + css.width / 2));
        el.setAttribute("cy", String(css.top + css.height / 2));
        el.setAttribute("rx", String(Math.max(1, css.width / 2)));
        el.setAttribute("ry", String(Math.max(1, css.height / 2)));
        el.setAttribute("fill", color);
        el.setAttribute("fill-opacity", "0.12");
        el.setAttribute("stroke", color);
        el.setAttribute("stroke-width", "2");
        el.setAttribute("stroke-dasharray", a.ann_type === "ellipse" ? "4 3" : "0");
        svg.appendChild(el);
      } else if (a.ann_type === "arrow" || pos.shape === "arrow") {
        const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
        line.setAttribute("x1", String(css.left));
        line.setAttribute("y1", String(css.top + css.height));
        line.setAttribute("x2", String(css.left + css.width));
        line.setAttribute("y2", String(css.top));
        line.setAttribute("stroke", color);
        line.setAttribute("stroke-width", "2.5");
        line.setAttribute("marker-end", "url(#sohei-arrow)");
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
      } else if (a.ann_type === "comment") {
        const el = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        el.setAttribute("x", String(css.left));
        el.setAttribute("y", String(css.top));
        el.setAttribute("width", String(css.width));
        el.setAttribute("height", String(css.height));
        el.setAttribute("fill", color);
        el.setAttribute("fill-opacity", "0.2");
        el.setAttribute("stroke", color);
        el.setAttribute("stroke-width", "1.5");
        svg.appendChild(el);
        // pin
        const pin = document.createElementNS("http://www.w3.org/2000/svg", "circle");
        pin.setAttribute("cx", String(css.left + 8));
        pin.setAttribute("cy", String(css.top + 8));
        pin.setAttribute("r", "7");
        pin.setAttribute("fill", color);
        svg.appendChild(pin);
      } else {
        // highlight
        const el = document.createElement("div");
        el.style.position = "absolute";
        el.style.left = `${css.left}px`;
        el.style.top = `${css.top}px`;
        el.style.width = `${css.width}px`;
        el.style.height = `${css.height}px`;
        el.style.background = color;
        el.style.opacity = "0.4";
        el.style.mixBlendMode = "multiply";
        el.style.pointerEvents = "none";
        el.title = a.content || "highlight";
        overlay.appendChild(el);
      }
    }
  }
  overlay.appendChild(svg);
}

function paintAllOverlays() {
  if (drawingActive) return;
  for (const v of pageViews.value) paintOverlay(v);
}

function updateOverlayCursors() {
  const cur = annotations.mode === "none" ? "default" : "crosshair";
  for (const v of pageViews.value) v.overlay.style.cursor = cur;
}

function localXY(ev: MouseEvent, overlay: HTMLDivElement) {
  const r = overlay.getBoundingClientRect();
  return { x: ev.clientX - r.left, y: ev.clientY - r.top };
}

function askComment(): Promise<string | null> {
  commentText.value = "";
  commentOpen.value = true;
  return new Promise((resolve) => {
    commentResolve = resolve;
  });
}

function submitComment() {
  const t = commentText.value.trim();
  commentOpen.value = false;
  commentResolve?.(t || null);
  commentResolve = null;
}

function cancelComment() {
  commentOpen.value = false;
  commentResolve?.(null);
  commentResolve = null;
}

function cleanupDrag(removeDom: boolean) {
  if (removeDom) {
    if (rubberEl?.parentElement) rubberEl.parentElement.removeChild(rubberEl);
    if (strokeSvg?.parentElement) strokeSvg.parentElement.removeChild(strokeSvg);
  }
  rubberEl = null;
  strokeSvg = null;
  strokeCss = [];
  dragStart = null;
  dragRect = null;
  drawingActive = false;
  window.removeEventListener("mousemove", onWindowMove);
  window.removeEventListener("mouseup", onWindowUp);
}

function onOverlayDown(ev: MouseEvent, view: PageView) {
  if (annotations.mode === "none" || ev.button !== 0) return;
  ev.preventDefault();
  ev.stopPropagation();

  cleanupDrag(true);
  drawingActive = true;

  const { x, y } = localXY(ev, view.overlay);
  dragStart = { pageNum: view.pageNum, x, y, view };
  dragRect = null;

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
  } else {
    rubberEl = document.createElement("div");
    rubberEl.style.position = "absolute";
    rubberEl.style.left = `${x}px`;
    rubberEl.style.top = `${y}px`;
    rubberEl.style.width = "0";
    rubberEl.style.height = "0";
    rubberEl.style.border = "1.5px dashed var(--accent)";
    rubberEl.style.background = "color-mix(in srgb, var(--accent) 18%, transparent)";
    rubberEl.style.pointerEvents = "none";
    rubberEl.style.zIndex = "5";
    if (annotations.mode === "ellipse") rubberEl.style.borderRadius = "50%";
    if (annotations.mode === "highlight") {
      rubberEl.style.border = "none";
      rubberEl.style.background = annotations.activeColor;
      rubberEl.style.opacity = "0.35";
    }
    view.overlay.appendChild(rubberEl);
  }

  window.addEventListener("mousemove", onWindowMove);
  window.addEventListener("mouseup", onWindowUp);
}

function onWindowMove(ev: MouseEvent) {
  if (!dragStart) return;
  const view = dragStart.view;
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
    path.setAttribute("stroke-width", "2.5");
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

async function onWindowUp(_ev: MouseEvent) {
  if (!dragStart) {
    cleanupDrag(true);
    return;
  }
  const view = dragStart.view;
  const mode = annotations.mode;
  const rect = dragRect;
  const pts = strokeCss.slice();

  // detach window listeners first
  window.removeEventListener("mousemove", onWindowMove);
  window.removeEventListener("mouseup", onWindowUp);

  if (rubberEl?.parentElement) rubberEl.parentElement.removeChild(rubberEl);
  if (strokeSvg?.parentElement) strokeSvg.parentElement.removeChild(strokeSvg);
  rubberEl = null;
  strokeSvg = null;
  dragStart = null;
  dragRect = null;
  strokeCss = [];
  drawingActive = false;

  if (mode === "none") return;

  if (mode === "drawing") {
    if (pts.length < 2) return;
    const pdfPts = pts.map((p) => {
      const [px, py] = view.viewport.convertToPdfPoint(p.x, p.y);
      return { x: px, y: py };
    });
    await annotations.create({
      document_id: props.documentId,
      ann_type: "drawing",
      page: view.pageNum,
      position_json: JSON.stringify({ page: view.pageNum, points: pdfPts }),
      content: "рисунок",
      color: annotations.activeColor,
    });
    paintAllOverlays();
    return;
  }

  if (!rect || rect.w < 1 || rect.h < 1) return;

  let content: string | null = null;
  let selectedText: string | null = null;
  let contextBefore: string | null = null;
  let contextAfter: string | null = null;
  if (mode === "comment") {
    content = await askComment();
    if (!content) return;
  } else if (mode === "highlight") {
    const anchor = await tryExtractText(view.pageNum, rect);
    selectedText = anchor?.selectedText ?? null;
    contextBefore = anchor?.contextBefore ?? null;
    contextAfter = anchor?.contextAfter ?? null;
    content = selectedText || "выделение";
  } else {
    content =
      mode === "ellipse"
        ? "овал"
        : mode === "rect"
          ? "прямоугольник"
          : mode === "arrow"
            ? "стрелка"
            : mode;
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
    selected_text: selectedText,
    context_before: contextBefore,
    context_after: contextAfter,
  });
  paintAllOverlays();
}

interface PdfTextAnchor {
  selectedText: string;
  contextBefore: string;
  contextAfter: string;
}

async function tryExtractText(pageNum: number, rect: PdfRect): Promise<PdfTextAnchor | null> {
  if (!pdfDoc) return null;
  try {
    const page = await pdfDoc.getPage(pageNum);
    const content = await page.getTextContent();
    const parts: string[] = [];
    const pageParts: string[] = [];
    for (const item of content.items as { str?: string; transform?: number[]; width?: number }[]) {
      if (!item.str || !item.transform) continue;
      pageParts.push(item.str);
      const x = item.transform[4];
      const y = item.transform[5];
      // rough hit-test in PDF space
      if (
        x >= rect.x - 2 &&
        x <= rect.x + rect.w + 2 &&
        y >= rect.y - 2 &&
        y <= rect.y + rect.h + 20
      ) {
        parts.push(item.str);
      }
    }
    const selectedText = parts.join(" ").replace(/\s+/g, " ").trim();
    if (!selectedText) return null;
    const pageText = pageParts.join(" ").replace(/\s+/g, " ").trim();
    const start = pageText.indexOf(selectedText);
    return {
      selectedText,
      contextBefore: start >= 0 ? pageText.slice(Math.max(0, start - 120), start) : "",
      contextAfter:
        start >= 0 ? pageText.slice(start + selectedText.length, start + selectedText.length + 120) : "",
    };
  } catch {
    return null;
  }
}

function bumpScale(delta: number) {
  const next = Math.min(3, Math.max(0.5, +(scale.value + delta).toFixed(2)));
  scale.value = next;
}

watch(() => annotations.items, () => paintAllOverlays(), { deep: true });
watch(() => annotations.mode, () => updateOverlayCursors());

watch(
  () => [props.path, props.documentId, props.cachePath] as const,
  () => {
    void loadPdf();
  },
);

watch(scale, () => {
  if (!pdfDoc || loading.value) return;
  if (scaleTimer) clearTimeout(scaleTimer);
  applyZoomPreview();
  // Render once after interaction settles. Until then the existing canvases
  // animate smoothly on the compositor.
  scaleTimer = setTimeout(() => {
    void renderAllPagesPreservingScroll();
  }, ZOOM_RENDER_DEBOUNCE_MS);
});

function onScroll() {
  updateCurrentPageFromScroll();
}

onMounted(() => {
  void loadPdf();
  // Bind scroll after first paint (ref may be null in onMounted before stage shows)
  nextTick(() => {
    containerRef.value?.addEventListener("scroll", onScroll, { passive: true });
  });
});

onBeforeUnmount(() => {
  loadGen++;
  scaleGen++;
  renderAbort = true;
  if (scaleTimer) clearTimeout(scaleTimer);
  cleanupDrag(true);
  containerRef.value?.removeEventListener("scroll", onScroll);
  destroyPages();
  destroyPdfDoc();
});
</script>

<template>
  <div class="pdf-viewer">
    <div class="pdf-toolbar">
      <button class="btn" :disabled="scale <= 0.5 || rendering" @click="bumpScale(-0.15)">−</button>
      <span class="muted">{{ Math.round(scale * 100) }}%</span>
      <button class="btn" :disabled="scale >= 3 || rendering" @click="bumpScale(0.15)">+</button>
      <span class="muted page-stable">
        <template v-if="pageCount">
          стр. {{ currentPage }} / {{ pageCount }}
        </template>
        <template v-else>—</template>
      </span>
      <span v-if="rendering" class="muted" style="font-size: 0.8rem">{{ status || "…" }}</span>
      <button class="btn" :disabled="loading || rendering" @click="loadPdf">Reload</button>
    </div>

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

      <!-- Comment modal (prompt is blocked in Tauri) -->
      <div v-if="commentOpen" class="comment-modal" @keydown.esc="cancelComment">
        <div class="comment-card">
          <h3>Комментарий</h3>
          <textarea
            v-model="commentText"
            rows="4"
            placeholder="Текст заметки…"
            autofocus
            @keydown.meta.enter="submitComment"
            @keydown.ctrl.enter="submitComment"
          />
          <div class="toolbar" style="justify-content: flex-end; margin-top: 10px">
            <button class="btn" @click="cancelComment">Отмена</button>
            <button class="btn btn-primary" @click="submitComment">Сохранить</button>
          </div>
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
.page-stable {
  font-variant-numeric: tabular-nums;
  min-width: 7.5rem;
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
  overscroll-behavior: contain;
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
.comment-modal {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.45);
  padding: 20px;
}
.comment-card {
  width: min(420px, 100%);
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 16px;
  box-shadow: var(--shadow);
}
.comment-card h3 {
  margin: 0 0 10px;
  font-size: 1rem;
}
.comment-card textarea {
  width: 100%;
  resize: vertical;
  min-height: 90px;
}
</style>
