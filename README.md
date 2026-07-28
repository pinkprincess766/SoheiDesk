<p align="center">
  <img src="assets/brand/app-icon-1024.png" width="96" height="96" alt="SoheiDesk" />
</p>

<h1 align="center">SoheiDesk</h1>

<p align="center">
  <strong>Scientific reader & lab journal</strong><br />
  <sub>Offline-first · two UI modes · PDF · DOCX · FB2 · annotations</sub>
</p>

<p align="center">
  <a href="./README.ru.md">Русский</a>
  ·
  <a href="https://github.com/pinkprincess766/SoheiDesk/releases">Download</a>
  ·
  <a href="https://github.com/pinkprincess766/SoheiDesk">GitHub</a>
</p>

---

## What is it?

A **desktop app** for reading papers and keeping light research notes — no account, no cloud required for core work.

| Goal | Where |
|------|--------|
| Read PDF / DOCX / MD / FB2 / DjVu… | **Reader** (or Simple mode) |
| Find text again | **Search** / **Library** |
| Lab notes | **Journal** |
| Export reports | **Export** |

---

## Two modes (choose on first launch)

| **Simple** | **Normal** |
|------------|------------|
| Terminal-like: text first, few buttons | Full workspace |
| **Open · Edit · Settings · Close / Quit** | Sidebar: library, journal, RSS, Zotero… |
| PDF → extracted text (reflow) | PDF → page canvas |
| Highlight `⌥1` · Comment `⌥2` · hjkl / arrows · Enter | Mouse tools on pages |

Change later: **Settings → UI mode**, or Simple → Settings → “Обычный”.

---

## Install

1. Open **[Releases](https://github.com/pinkprincess766/SoheiDesk/releases)**
2. Download for your OS:

| OS | File |
|----|------|
| macOS (Apple Silicon or Intel) | `SoheiDesk_…_universal.dmg` or arm64 DMG |
| Windows | `.msi` / setup `.exe` |
| Linux | `.AppImage` / `.deb` |

3. Install and open **SoheiDesk**.

**Open with:** right‑click a PDF → *Open With* → SoheiDesk.

### Optional (DjVu text)

```bash
brew install djvulibre   # provides djvutxt
```

Without it, DjVu still opens with a short help note; PDF/DOCX/MD/FB2 work as usual.

---

## First steps

### Simple
1. Pick **Простой / Simple** at launch  
2. **Open** a document  
3. `⌥1` highlight · `⌥2` comment · **hjkl** · **Enter** · **Esc**  
4. **Quit** closes the app  

### Normal
1. Pick **Обычный / Normal**  
2. **Reader** → Open file (`⌘/Ctrl+O`)  
3. Annotate in the right panel  
4. Journal / Export / Search as needed  

Shortcuts (Normal): `⌘O` open · `⌘F` search · `⌘J` journal · `⌘,` settings  

---

## Privacy

- Files stay on **your disk**  
- App data: OS Application Support (SQLite + media cache)  
- No account for core reading  

---

## Build from source

```bash
# Need: Rust, Node 20+, pnpm
git clone https://github.com/pinkprincess766/SoheiDesk.git
cd SoheiDesk
pnpm install
pnpm app:dev          # full Tauri + Vite
pnpm app:dev:ui       # UI only (no Rust rebuild)
pnpm app:build        # installers
pnpm clean            # drop multi‑GB target/ (safe)
```

**Disk:** `src-tauri/target` can grow to several GB during builds — it is gitignored. Run `pnpm clean` anytime.

**Icons:** `assets/brand/app-icon-1024.png` → `pnpm app:icons`

---

## Stack

- **UI:** Vue 3 + TypeScript + Pinia  
- **Shell:** Tauri 2  
- **Core:** Rust (parsers, SQLite, PDF text, FS)  
- **Fonts:** [DM Sans](https://fonts.google.com/specimen/DM+Sans) · [JetBrains Mono](https://fonts.google.com/specimen/JetBrains+Mono) (OFL)  

PDF pages use pdf.js in Normal mode; Simple mode uses Rust/pdf text extract (cached).

---

## Formats

PDF · Markdown · TXT · DOCX · EPUB · HTML · TeX · **FB2** · **DjVu** (text via DjVuLibre)

---

## License

See [LICENSE](./LICENSE).

<p align="center">
  <sub>Built for people who read papers — not another SaaS login.</sub>
</p>
