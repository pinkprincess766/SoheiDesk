<p align="center">
  <img src="assets/brand/app-icon-1024.png" width="96" height="96" alt="SoheiDesk" />
</p>

<h1 align="center">SoheiDesk</h1>

<p align="center">
  <strong>Научный ридер и лабораторный журнал</strong><br />
  <sub>Офлайн · два режима · PDF · DOCX · FB2 · аннотации</sub>
</p>

<p align="center">
  <a href="./README.md">English</a>
  ·
  <a href="https://github.com/pinkprincess766/SoheiDesk/releases">Скачать</a>
  ·
  <a href="https://github.com/pinkprincess766/SoheiDesk">GitHub</a>
</p>

---

## Зачем

Программа **на компьютере**: читать статьи, помечать важное, вести короткий журнал. Без аккаунта для основной работы.

| Задача | Куда |
|--------|------|
| Читать PDF / DOCX / MD / FB2 / DjVu… | **Ридер** или **Простой** режим |
| Найти текст | **Поиск** / **Библиотека** |
| Заметки по работе | **Журнал** |
| Отчёт | **Экспорт** |

---

## Два режима (выбор при первом запуске)

| **Простой** | **Обычный** |
|-------------|-------------|
| Как терминал: сразу **текст**, мало кнопок | Полный рабочий стол |
| **Open · Edit · Settings · Close / Quit** | Сайдбар: библиотека, журнал, RSS… |
| PDF → извлечённый текст | PDF → страницы |
| Выделение `⌥1` · комментарий `⌥2` · hjkl · Enter | Инструменты мышью |

Сменить: **Настройки → Режим**, или Simple → Settings → «Обычный».

### Простой режим — клавиши

| Клавиша | Действие |
|---------|----------|
| `⌥1` | Режим выделения |
| `⌥2` | Режим комментария |
| `h j k l` / стрелки | Двигать курсор / выделение |
| `Enter` | Подтвердить |
| `Esc` | Отмена |
| **Quit** | Выйти из приложения |
| **Close** | Закрыть только файл |

---

## Установка

1. **[Releases](https://github.com/pinkprincess766/SoheiDesk/releases)**  
2. Файл под вашу ОС (DMG / MSI / AppImage)  
3. Установить и запустить **SoheiDesk**  

**Открыть с помощью:** ПКМ по PDF → SoheiDesk.

### DjVu (текст)

```bash
brew install djvulibre
```

Без этого DjVu откроется с подсказкой; PDF/DOCX/MD/FB2 работают и так.

---

## С чего начать

**Простой:** режим Simple → Open → читать текст → `⌥1` / `⌥2` → Quit.  

**Обычный:** Reader → Open (`⌘O`) → аннотации справа → Журнал / Экспорт.

---

## Приватность

Файлы на **вашем диске**. Данные приложения — в Application Support. Аккаунт не обязателен.

---

## Сборка

```bash
pnpm install
pnpm app:dev       # разработка
pnpm app:dev:ui    # только UI, без пересборки Rust
pnpm app:build     # установщики
pnpm clean         # удалить target/ (гигабайты кэша)
```

Папка `src-tauri/target` при сборке может занимать много места — в git не входит.

---

## Стек

Vue 3 · Tauri 2 · Rust (парсеры, SQLite, текст PDF) · шрифты DM Sans + JetBrains Mono.

---

## Форматы

PDF · Markdown · TXT · DOCX · EPUB · HTML · TeX · **FB2** · **DjVu**

---

## Лицензия

[LICENSE](./LICENSE).
