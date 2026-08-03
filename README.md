<p align="center">
  <img src="resources/brand/app-icon-1024.png" width="96" height="96" alt="SoheiDesk" />
</p>

<h1 align="center">SoheiDesk</h1>

<p align="center">
  A local desktop workspace for reading research documents, making annotations,
  and keeping lab notes.
</p>

<p align="center">
  <a href="./docs/translations/README.ru.md">Русский</a>
  ·
  <a href="https://github.com/pinkprincess766/SoheiDesk/releases">Releases</a>
  ·
  <a href="#build-from-source">Build from source</a>
</p>

<p align="center">
  <img src="docs/images/pdf-zoom.gif" width="900" alt="Opening and zooming a PDF in SoheiDesk" />
</p>

SoheiDesk is an early-stage desktop app for a straightforward workflow:
open a paper, mark useful passages, write down what you did, and export the
result. Core reading and journal data stay on the computer; an account is not
required.

## What works today

| Area | Current capability |
|------|--------------------|
| Reader | Page-based PDF view and reflowed text for common document formats |
| Annotations | Highlights, comments, freehand strokes, rectangles, ellipses, and arrows |
| Journal | Built-in and custom templates, attachments, preview, and export |
| Draft safety | Journal and template drafts are autosaved to SQLite and can be restored explicitly |
| Backups | Daily, pre-migration, and manual backups with verified restore and an automatic emergency copy |
| Library and search | Local document history plus full-text search across documents and journal entries |
| Export | Markdown, HTML, Typst, LaTeX, and DOCX output |
| References | DOI lookup, arXiv/PubMed search, bibliography export, and read-only Zotero import |

The normal path through the app is deliberately small:

1. Open a paper in **Reader**.
2. Add a highlight or comment without changing the source file.
3. Record the result in **Journal**.
4. Preview and export the note when it is ready to share.

## Screenshots

| PDF reader and annotations | Lab journal templates |
|:--------------------------:|:---------------------:|
| <img src="docs/images/reader.png" width="560" alt="PDF reader with annotation tools" /> | <img src="docs/images/journal.png" width="560" alt="Lab journal template picker" /> |

## Two interface modes

**Normal mode** exposes the complete workspace: reader, library, journal,
search, export, references, RSS, Zotero, OCR, and settings.

**Simple mode** is a keyboard-oriented reflow reader with a small toolbar.
It is useful when the document text matters more than the original page
layout. Change modes at first launch or later in Settings.

Common shortcuts in Normal mode:

| Shortcut | Action |
|----------|--------|
| `⌘/Ctrl+O` | Open a document |
| `⌘/Ctrl+F` | Search |
| `⌘/Ctrl+J` | Open the journal |
| `⌘/Ctrl+,` | Open settings |

## Install

Published builds, when available, are attached to
[GitHub Releases](https://github.com/pinkprincess766/SoheiDesk/releases).
Download the asset that matches your operating system:

- macOS: `.dmg`
- Windows: `.msi` or setup `.exe`
- Linux: `.AppImage` or `.deb`

The application is not currently code-signed for every platform, so the
operating system may show an unknown-developer warning. If a build is not
available for your platform, use the source build below.

## Formats

| Format | How it is opened |
|--------|------------------|
| PDF | Original pages through PDF.js; extracted text is also cached for Simple mode and search |
| DOCX, EPUB, FB2, HTML, Markdown | Converted to a reflowed reading view |
| TXT, TeX | Plain-text reading view |
| DjVu | Text extraction through the optional `djvutxt` command from DjVuLibre |

Scanned PDFs need OCR before their text can be searched. Encrypted PDFs are
not supported at the moment.

## Data and network use

- Documents, annotations, journal entries, drafts, and settings are stored
  locally.
- SoheiDesk does not require an account for core work.
- Network access is used only when you request an online feature such as DOI,
  arXiv, PubMed, or RSS.
- Zotero import opens the database read-only.
- OCR, DjVu support, plugins, and ChromaTsvet integration can invoke tools
  installed on the computer.
- LAN sharing is optional and intended for a trusted local network, not the
  public internet.
- Backups are stored under `backups` in the application data directory. A
  backup contains the main SQLite database, app-managed media and attachments,
  settings, user templates, and a versioned checksum manifest. The rebuildable
  search index is intentionally excluded.

When SoheiDesk is running, it creates at most one automatic backup per local
day and keeps 7 daily plus 4 older weekly copies. It also creates a backup
before each database migration and an emergency copy before restore. Manual
creation, backup history, and restore are available in **Settings**. Every
restore verifies all SHA-256 checksums and runs SQLite `integrity_check` before
current data is changed. The archive layout is documented in
[docs/architecture/backup-format.md](./docs/architecture/backup-format.md).

Database upgrades fail closed. SoheiDesk rejects unknown newer or malformed
schema histories, backs up an existing database before each migration, applies
each step transactionally, and checks SQLite both before and after commit. The
contract and failure behavior are documented in
[docs/architecture/database-migrations.md](./docs/architecture/database-migrations.md).

User-facing exports are written to a verified temporary file beside the chosen
destination, synchronized, and atomically installed only after validation. A
failed or interrupted export therefore leaves the previous file unchanged. The
contract covers Markdown, HTML, Typst, LaTeX, DOCX, BibTeX, annotation exports,
and JSON templates; implementation details are documented in
[docs/architecture/atomic-file-writes.md](./docs/architecture/atomic-file-writes.md).

SoheiDesk is under active development and is not a validated electronic
laboratory notebook. Source documents opened from locations outside the app
data directory are not guaranteed to be copied into a SoheiDesk backup; keep
normal backups of those originals as well.

## Build from source

Requirements: Rust, Node.js 20 or newer, and pnpm.

```bash
git clone https://github.com/pinkprincess766/SoheiDesk.git
cd SoheiDesk
pnpm install
pnpm app:dev
```

Useful commands:

```bash
pnpm test:frontend   # frontend regression and security tests
pnpm build           # TypeScript check + production UI build
pnpm app:build       # desktop installers

cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

`src-tauri/target` can grow to several gigabytes during Rust builds. Run
`pnpm clean` to remove generated build artifacts.

## Project layout

The repository keeps runtime code, test material, documentation, and source
assets separate:

- `frontend/` - Vue application, Vite configuration, and static web assets
- `src-tauri/` - Rust application core and Tauri desktop configuration
- `tests/fixtures/` - sample documents used only by automated tests
- `resources/brand/` - source artwork used to generate application icons
- `docs/architecture/` - storage formats and internal contracts
- `docs/releases/` - version-specific release notes
- `docs/translations/` - translated project documentation

See [the project structure guide](./docs/project-structure.md) before adding a
new top-level directory.

## Current scope

The present product is a local reader and research journal. Semantic AI
search, automatic spectroscopy reports, voice control, and knowledge maps are
ideas for later work, not current features.

## Stack

Vue 3 · TypeScript · Pinia · Tauri 2 · Rust · SQLite · PDF.js

## License

[MIT](./LICENSE)
