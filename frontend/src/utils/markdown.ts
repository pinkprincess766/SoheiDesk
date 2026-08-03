/** Minimal Markdown → HTML. All ![alt](url) become real <img> tags. */

export function renderMarkdown(src: string): string {
  const slots: string[] = [];

  // Protect EVERY markdown image (data:, asset://, https://, sohei-file already resolved, etc.)
  // Use [^)\n]+ so spaces in asset URLs (Application Support) still match.
  const withImgs = src.replace(/!\[([^\]]*)\]\(([^)\n]+)\)/g, (_m, alt: string, uri: string) => {
    const i = slots.length;
    const safeAlt = escapeHtml(alt || "image");
    const raw = String(uri).trim().replace(/"/g, "%22");
    // Block javascript:/data:text HTML vectors in img src (allow data:image/*, asset, http, sohei-file)
    if (!isSafeMediaUri(raw)) {
      return escapeHtml(`![${alt}](${uri})`);
    }
    slots.push(
      `<img src="${raw}" alt="${safeAlt}" class="doc-image" loading="lazy" />`,
    );
    return `\n\n%%IMG${i}%%\n\n`;
  });

  const escaped = escapeHtml(withImgs);
  const lines = escaped.split(/\r?\n/);
  const out: string[] = [];
  let inCode = false;
  let codeBuf: string[] = [];
  let inList = false;

  const closeList = () => {
    if (inList) {
      out.push("</ul>");
      inList = false;
    }
  };

  for (const line of lines) {
    const imgSlot = line.trim().match(/^%%IMG(\d+)%%$/);
    if (imgSlot) {
      closeList();
      out.push(slots[Number(imgSlot[1])] || "");
      continue;
    }

    if (line.startsWith("```")) {
      if (inCode) {
        out.push(`<pre><code>${codeBuf.join("\n")}</code></pre>`);
        codeBuf = [];
        inCode = false;
      } else {
        closeList();
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      codeBuf.push(line);
      continue;
    }

    if (/^###\s+/.test(line)) {
      closeList();
      out.push(`<h3>${inline(line.replace(/^###\s+/, ""))}</h3>`);
    } else if (/^##\s+/.test(line)) {
      closeList();
      out.push(`<h2>${inline(line.replace(/^##\s+/, ""))}</h2>`);
    } else if (/^#\s+/.test(line)) {
      closeList();
      out.push(`<h1>${inline(line.replace(/^#\s+/, ""))}</h1>`);
    } else if (/^&gt;\s*/.test(line)) {
      closeList();
      out.push(`<blockquote>${inline(line.replace(/^&gt;\s*/, ""))}</blockquote>`);
    } else if (/^[-*]\s+/.test(line)) {
      if (!inList) {
        out.push("<ul>");
        inList = true;
      }
      out.push(`<li>${inline(line.replace(/^[-*]\s+/, ""))}</li>`);
    } else if (line.trim() === "") {
      closeList();
      out.push("<br/>");
    } else {
      closeList();
      const withSlots = line.replace(/%%IMG(\d+)%%/g, (_m, n) => slots[Number(n)] || "");
      // if line became pure img already, don't wrap extra
      if (withSlots.includes("<img ")) {
        out.push(withSlots);
      } else {
        out.push(`<p>${inline(withSlots)}</p>`);
      }
    }
  }
  if (inCode) {
    out.push(`<pre><code>${codeBuf.join("\n")}</code></pre>`);
  }
  closeList();
  return out.join("\n");
}

function inline(s: string): string {
  if (s.includes("<img ")) {
    return s
      .replace(/\$\$([^$]+)\$\$/g, '<span class="formula-block">\\[$1\\]</span>')
      .replace(/\$([^$]+)\$/g, '<span class="formula-inline">\\($1\\)</span>')
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\*([^*]+)\*/g, "<em>$1</em>");
  }
  return s
    .replace(/\$\$([^$]+)\$\$/g, '<span class="formula-block">\\[$1\\]</span>')
    .replace(/\$([^$]+)\$/g, '<span class="formula-inline">\\($1\\)</span>')
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, text: string, href: string) => {
      const h = String(href).trim();
      if (!isSafeLinkHref(h)) {
        return escapeHtml(text);
      }
      return `<a href="${h.replace(/"/g, "%22")}" target="_blank" rel="noreferrer noopener">${text}</a>`;
    });
}

/** Safe image sources for local viewer (docx/pdf assets, remote figures). */
function isSafeMediaUri(uri: string): boolean {
  const u = uri.trim().toLowerCase();
  if (u.startsWith("javascript:") || u.startsWith("vbscript:")) return false;
  if (u.startsWith("data:")) {
    // no svg+xml (scriptable); only raster data URIs from docx extract
    return /^data:image\/(png|jpe?g|gif|webp|bmp);/i.test(uri.trim());
  }
  return (
    u.startsWith("http://") ||
    u.startsWith("https://") ||
    u.startsWith("asset:") ||
    u.startsWith("asset://") ||
    u.startsWith("https://asset.localhost") ||
    u.startsWith("sohei-file://") ||
    u.startsWith("blob:") ||
    u.startsWith("/") // relative / asset paths after convertFileSrc
  );
}

function isSafeLinkHref(href: string): boolean {
  const h = href.trim().toLowerCase();
  if (h.startsWith("javascript:") || h.startsWith("vbscript:") || h.startsWith("data:")) {
    return false;
  }
  return (
    h.startsWith("http://") ||
    h.startsWith("https://") ||
    h.startsWith("mailto:") ||
    h.startsWith("#") ||
    h.startsWith("/")
  );
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
