import assert from "node:assert/strict";
import test from "node:test";

import { renderMarkdown } from "../src/utils/markdown.ts";

test("escapes raw HTML and script tags", () => {
  const html = renderMarkdown('<script>alert("x")</script>');
  assert.doesNotMatch(html, /<script>/);
  assert.match(html, /&lt;script&gt;/);
});

test("blocks executable link protocols", () => {
  const html = renderMarkdown("[open](javascript:evil)");
  assert.doesNotMatch(html, /href=/);
  assert.doesNotMatch(html, /javascript:/);
  assert.match(html, />open</);
});

test("blocks scriptable and non-image data URIs", () => {
  const svg = renderMarkdown("![x](data:image/svg+xml;base64,PHN2Zz4=)");
  const html = renderMarkdown("![x](data:text/html;base64,PHNjcmlwdD4=)");
  assert.doesNotMatch(svg, /<img/);
  assert.doesNotMatch(html, /<img/);
});

test("allows raster images and applies lazy loading", () => {
  const html = renderMarkdown("![diagram](data:image/png;base64,AAAA)");
  assert.match(html, /<img /);
  assert.match(html, /loading="lazy"/);
  assert.match(html, /alt="diagram"/);
});

test("external links use opener isolation", () => {
  const html = renderMarkdown("[paper](https://example.com/paper)");
  assert.match(html, /target="_blank"/);
  assert.match(html, /rel="noreferrer noopener"/);
});

test("escapes quotes in image attributes", () => {
  const html = renderMarkdown('![a"b](https://example.com/a"b.png)');
  assert.doesNotMatch(html, /alt="a"b"/);
  assert.match(html, /alt="a&quot;b"/);
  assert.match(html, /%22/);
});
