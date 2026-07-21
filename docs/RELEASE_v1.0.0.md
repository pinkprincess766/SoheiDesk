# SoheiDesk 1.0.0

Первый полноценный релиз приложения для компьютера.

---

## 🇷🇺 Для пользователей (простыми словами)

### Что это
**SoheiDesk** — программа, в которой можно:
- открывать документы (PDF, Word, текст и др.);
- делать пометки (выделение, комментарии, рисование на PDF);
- вести **журнал** (дневник работ) по шаблонам;
- **экспортировать** отчёты (Markdown, HTML, Typst, LaTeX, Word);
- искать по своим материалам;
- при желании — литература (DOI, arXiv…), RSS, Zotero.

**Не нужно** уметь программировать и открывать терминал.

### Как установить

1. Ниже, в блоке **Assets**, скачайте файл **для вашей системы**.
2. Установите как обычную программу (инструкция в [README.ru.md](https://github.com/pinkprincess766/SoheiDesk/blob/main/README.ru.md)).

| Ваш компьютер | Какой файл |
|---------------|------------|
| **Mac** (M1/M2/M3/M4 или Intel) | `SoheiDesk_1.0.0_universal.dmg` (предпочтительно) |
| **Mac** только Apple Silicon | `SoheiDesk_1.0.0_aarch64.dmg` |
| **Windows** | `…msi` или `…setup.exe` |
| **Linux** | `…AppImage` или `…deb` |

### Mac: «неизвестный разработчик»
Правый клик по SoheiDesk → **Открыть** → снова **Открыть**.  
Подробно: [README.ru.md](https://github.com/pinkprincess766/SoheiDesk/blob/main/README.ru.md)

### Windows: SmartScreen
**Подробнее** → **Выполнить в любом случае**.

### С чего начать после установки
1. Откройте **Ридер** → **Открыть файл…** → выберите PDF.  
2. Попробуйте **Журнал** → новая запись.  
3. При необходимости — **Экспорт**.

Полная инструкция: https://github.com/pinkprincess766/SoheiDesk/blob/main/README.ru.md

---

## 🇬🇧 English (short)

**SoheiDesk 1.0** is a desktop app for reading documents, annotations, a lab-style journal, export, and search — **no coding required**.

1. Download the installer for your OS from **Assets** below.  
2. Install like any normal app.  
3. Full guide: [README.md](https://github.com/pinkprincess766/SoheiDesk/blob/main/README.md) · Russian: [README.ru.md](https://github.com/pinkprincess766/SoheiDesk/blob/main/README.ru.md)

| OS | File |
|----|------|
| Mac (Intel + Apple Silicon) | `SoheiDesk_1.0.0_universal.dmg` |
| Windows | `.msi` / setup `.exe` |
| Linux | `.AppImage` / `.deb` |

---

## Что внутри 1.0 (кратко)

- Ридер: PDF (в т.ч. рисование), MD/TXT/DOCX/EPUB/HTML/TEX  
- Аннотации отдельно от файлов  
- Журнал + шаблоны + экспорт отчётов  
- Поиск по библиотеке  
- Литература (DOI / arXiv / PubMed), Zotero, RSS  
- OCR (если установлен Tesseract)  
- Опционально: LAN share, плагины-парсеры  

---

## Для разработчиков

```bash
pnpm install
pnpm app:build
```

CI: Actions → **Build all platforms**  
Сборка всех ОС: [СБОРКА-ВСЕ-ОС.md](https://github.com/pinkprincess766/SoheiDesk/blob/main/СБОРКА-ВСЕ-ОС.md)

**Commit / tag:** `v1.0.0`
