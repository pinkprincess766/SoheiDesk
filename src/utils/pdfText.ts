/** Extract plain text from PDF bytes via pdf.js (Simple mode reflow). */
import * as pdfjs from "pdfjs-dist";

pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  "/pdf.worker.min.mjs",
  window.location.href,
).toString();

export async function extractPdfPlainText(
  data: Uint8Array,
  onProgress?: (page: number, total: number) => void,
): Promise<string> {
  const copy = Uint8Array.from(data);
  const ab = copy.buffer.slice(copy.byteOffset, copy.byteOffset + copy.byteLength);
  const doc = await pdfjs.getDocument({
    data: new Uint8Array(ab),
    useSystemFonts: true,
    disableStream: true,
    disableRange: true,
    stopAtErrors: false,
  }).promise;

  const parts: string[] = [];
  const total = doc.numPages;
  for (let i = 1; i <= total; i++) {
    onProgress?.(i, total);
    try {
      const page = await doc.getPage(i);
      const content = await page.getTextContent();
      const line = (content.items as { str?: string }[])
        .map((it) => it.str || "")
        .join(" ")
        .replace(/\s+/g, " ")
        .trim();
      if (line) parts.push(line);
    } catch {
      /* skip bad page */
    }
  }
  try {
    const d = doc as unknown as { destroy?: () => Promise<void> | void; cleanup?: () => void };
    d.cleanup?.();
    await d.destroy?.();
  } catch {
    /* */
  }
  return parts.join("\n\n");
}
