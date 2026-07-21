# SoheiDesk

**A normal desktop app** for your computer: open documents (PDF and more), highlight them, keep a simple work diary, and save reports.

You do **not** need to know programming.  
You do **not** need a terminal or command line.  
You download an installer, install the app, and open it **like any other program** (browser, Word, etc.).

> **Русская версия (подробная, для пользователей):**  
> **[README.ru.md](./README.ru.md)** ← start here if you prefer Russian.

<p align="center">
  <img src="assets/brand/app-icon-1024.png" width="128" alt="SoheiDesk icon" />
</p>

**Repository:** [github.com/pinkprincess766/SoheiDesk](https://github.com/pinkprincess766/SoheiDesk)  
**Download the app:** [Releases](https://github.com/pinkprincess766/SoheiDesk/releases)

---

## Table of contents

1. [What is this, in plain language](#1-what-is-this-in-plain-language)  
2. [What you need](#2-what-you-need)  
3. [How to download](#3-how-to-download)  
4. [Install on Mac](#4-install-on-mac)  
5. [Install on Windows](#5-install-on-windows)  
6. [Install on Linux](#6-install-on-linux)  
7. [First launch](#7-first-launch)  
8. [How to use (step by step)](#8-how-to-use-step-by-step)  
9. [Common problems](#9-common-problems)  
10. [FAQ](#10-faq)  
11. [Privacy and your data](#11-privacy-and-your-data)  
12. [For people who build the app](#12-for-people-who-build-the-app)  

---

## 1. What is this, in plain language

SoheiDesk is **one program on your computer** where you can:

| Goal | Where in SoheiDesk |
|------|--------------------|
| Open a paper or file | **Reader** → “Open file…” |
| Highlight / draw notes | Right-hand panel in the reader |
| Write what you did today | **Journal** |
| Save a report as a file | **Export** |
| Search your materials | **Search** |

It works **mostly offline**.  
Internet is only needed for optional features (for example looking up a paper by DOI, or RSS feeds).

---

## 2. What you need

- A computer: **Mac**, **Windows**, or **Linux**
- Disk space: about **50–100 MB** for the app (plus your own files)
- An installer from **[Releases](https://github.com/pinkprincess766/SoheiDesk/releases)**

You do **not** need: coding skills, Python, Node, or a “developer environment”.

---

## 3. How to download

1. Open: **https://github.com/pinkprincess766/SoheiDesk/releases**  
2. Open the **latest** release (the one at the top).  
3. Under **Assets**, download the file for **your** system:

| Your computer | Download (typical names) |
|---------------|---------------------------|
| **Mac** (Apple Silicon **or** Intel) | `SoheiDesk_…_universal.dmg` or `soheidesk-macos….zip` |
| **Mac** Apple Silicon only (if no universal file) | `SoheiDesk_…_aarch64.dmg` |
| **Windows** | `.msi` or setup `.exe` / `soheidesk-windows….zip` |
| **Linux** | `.AppImage` or `.deb` / `soheidesk-linux….zip` |

> **Not sure which Mac you have?**  
> Apple menu → **About This Mac**.  
> If you see **Chip** → Apple Silicon. If **Intel** → Intel.  
> The **universal** file works for **both**.

5. Wait until the download finishes.  
6. Remember the folder (often **Downloads**).

---

## 4. Install on Mac

### Steps

1. In **Downloads**, find a file like  
   `SoheiDesk_0.4.0_universal.dmg`  
   (the version number may differ).
2. **Double-click** it.
3. A small window appears with the **SoheiDesk** icon and **Applications**.
4. **Drag** SoheiDesk **onto** Applications.
5. Wait for the copy to finish.
6. Open **Finder → Applications** and find **SoheiDesk**.
7. **Double-click** SoheiDesk.

Optional: keep it in the Dock — right-click the Dock icon → **Options → Keep in Dock**.

### If Mac says: “cannot be opened because it is from an unidentified developer”

This is **normal** if the app is not yet signed with an Apple Developer certificate.  
You did nothing wrong.

**Method 1 (easiest):**

1. Find SoheiDesk in **Applications**.  
2. **Right-click** (or Control-click) the icon.  
3. Choose **Open**.  
4. In the warning dialog, click **Open** again.  
5. Next time, a normal double-click is enough.

**Method 2:**

1. **System Settings → Privacy & Security**.  
2. Scroll down.  
3. If you see **Open Anyway** next to a SoheiDesk message, click it.

### If you only have a `.zip`

1. Double-click the zip to unpack.  
2. If you get a `.dmg`, open it (steps above).  
3. If you get `SoheiDesk.app`, drag it into **Applications**.

---

## 5. Install on Windows

1. Download the Windows file (`.msi` or setup `.exe`).  
2. **Double-click** it.  
3. If Windows asks for permission to make changes → **Yes**.  
4. Click **Next → Install**.  
5. Start **SoheiDesk** from the **Start** menu or desktop shortcut.

### If Windows says: “Windows protected your PC” (SmartScreen)

1. Click **More info**.  
2. Click **Run anyway**.  

This often happens for apps without a paid Microsoft code-signing certificate.  
Only install files from **this project’s Releases** page.

---

## 6. Install on Linux

### Option A — AppImage (often simplest)

1. Download the `…AppImage` file.  
2. Right-click → **Properties → Permissions**.  
3. Enable **“Allow executing file as program”**.  
4. Double-click the file.

If it still does not start, ask the person who gave you the link (or your admin) for one-time help.  
On some Ubuntu versions, `libfuse2` may be required for AppImage.

### Option B — `.deb` (Ubuntu / Debian)

1. Double-click the `.deb` or open it with your software installer.  
2. Click **Install**.  
3. Launch SoheiDesk from the applications menu.

---

## 7. First launch

You should see a window with a **left menu** (Reader, Library, Journal…).

1. Click **Reader**.  
2. Click **Open file…**.  
3. Pick any PDF or text file on your computer.  
4. It should open in the window.

If that works, installation succeeded.

---

## 8. How to use (step by step)

### 8.1. Open a document

1. Left menu: **Reader**.  
2. **Open file…**.  
3. Choose a file.  

Common formats: **PDF**, **Word (DOCX)**, plain **text**, **Markdown**, **HTML**, sometimes **EPUB**.

> SoheiDesk does **not** modify your original file during normal use.  
> Highlights and comments are stored **inside the app**, separately.

### 8.2. Annotations (highlight / comment)

1. Open a document.  
2. Use the panel on the right.  
3. Choose **Highlight** or **Comment**.  
4. For text files: select text with the mouse.  
5. For PDF: drag a rectangle / use drawing modes.

Annotations are saved automatically. Close the app and reopen the file — they should still be there.

### 8.3. Library

**Library** is a list of files you already opened in SoheiDesk.

- Open them again from here.  
- **Remove** only removes the entry from the list — it does **not** delete the file from your disk.

### 8.4. Journal (diary)

1. Left menu: **Journal**.  
2. **New entry** or pick a **template**.  
3. Fill in the fields and text.  
4. Click **Save**.

Useful for “what I did”, “what I measured”, “what I read”.

### 8.5. Export (save a report)

1. Left menu: **Export**.  
2. Choose one journal entry **or** a date range.  
3. Choose a format (e.g. Markdown, HTML…).  
4. **Preview**.  
5. **Save file…** and pick a folder on your computer.

### 8.6. Search

1. **Search**.  
2. Type a word.  
3. **Find**.  
4. Click a result to open it.

### 8.7. Other sections (optional)

| Section | Purpose |
|---------|---------|
| **Literature** | Look up paper info by DOI / search (needs internet) |
| **Zotero** | Import from the Zotero desktop app if you use it |
| **RSS** | Journal news feeds (needs internet) |
| **OCR** | Read text from an **image** (needs Tesseract installed separately) |
| **Plugins** | Advanced users |
| **Settings** | Theme, shortcuts help, optional LAN share |

If you do not need something — **ignore it**. Day-to-day use is **Reader + Journal**.

---

## 9. Common problems

### “App won’t open” (Mac)

- Use **right-click → Open** ([section 4](#4-install-on-mac)).  
- Make sure you moved the app into **Applications**, and you are not only launching it from an unmounted DMG window.

### “Windows blocked the app”

- **More info → Run anyway** ([section 5](#5-install-on-windows)).  
- Download only from this project’s Releases.

### “I can’t find my file”

- SoheiDesk does **not** scan your whole computer (for safety).  
- Always use **Open file…** and pick the file yourself.

### “My highlights disappeared”

- If the file was **renamed / moved / heavily edited**, the app may treat it as new.  
- Try opening the same path again from **Library** if it is listed there.

### “DOI / online features fail”

- Reading PDFs and the journal work **offline**.  
- DOI, arXiv, PubMed, RSS need **internet**.

### “OCR does nothing”

- You need **Tesseract** installed on the computer.  
- Without it, SoheiDesk still works — only image OCR is missing.

### Large PDF is slow

- Wait a moment after open.  
- Zoom out (−).  
- Close other heavy apps.

---

## 10. FAQ

**Is this a website?**  
No. It is a desktop app. After install, you do not need the internet for core use.

**Do I need an account?**  
No.

**Where is my data stored?**  
On **your** computer, in the app’s local data folder. Nothing is uploaded automatically.

**Phone / tablet?**  
Not supported. Computer only (Mac / Windows / Linux).

**Will it damage my PDFs?**  
No. Original files are not overwritten during normal use.

**Who do I contact if I’m stuck?**  
The person who sent you the Releases link (teacher, colleague, maintainer).  
Include: **your OS**, **what you clicked**, and a **screenshot** of the error text.

---

## 11. Privacy and your data

- Journal entries, annotations, and settings stay **local** on your machine.  
- **LAN share** in Settings is **optional** (shows a read-only page on your local network).  
  Do **not** turn it on unless you understand it. Avoid on public Wi‑Fi.  
- Only download installers from **this repository’s Releases**.

---

## 12. For people who build the app

End users should **ignore** this section.

```bash
pnpm install
pnpm app:dev      # development window
pnpm app:build    # installer for the current OS only
```

Build for macOS + Windows + Linux at once: GitHub Actions  
`.github/workflows/release.yml` → **Build all platforms**.

Details (Russian): [`СБОРКА-ВСЕ-ОС.md`](./СБОРКА-ВСЕ-ОС.md), [`КАК УСТАНОВИТЬ.md`](./КАК%20УСТАНОВИТЬ.md).

Full user guide in Russian: [`README.ru.md`](./README.ru.md).

---

<p align="center">
  <b>SoheiDesk</b> — open a file, mark it, save a note.<br/>
  Stuck? Start with <b>Reader</b> and <b>Open file…</b>
</p>
