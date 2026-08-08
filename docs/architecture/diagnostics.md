# Local diagnostics contract

SoheiDesk exposes a local **Status** page for inspecting application health
without sending telemetry or requiring a cloud service. Opening the page does
not create a network request.

## Reported state

The page reports:

- the application version, current database schema, and latest supported
  schema;
- the result of `PRAGMA quick_check(1)`;
- the latest successfully created readable backup;
- byte and file counts for SQLite (including WAL/SHM), attachments,
  app-managed media, and the rebuildable Tantivy index;
- a real PDF.js worker handshake performed in the webview;
- availability of Tesseract, DjVuLibre, and the configured ChromaTsvet binary;
- recent timestamps and fixed error categories.

Directory measurements never follow symbolic links. A partially inaccessible
tree is marked incomplete instead of being guessed or silently reported as
empty.

## Privacy boundary

Raw error strings are accepted only long enough to map them to a finite,
content-free category such as `PDF operation failed` or `Storage write failed`.
They are never written to disk. Log entries contain only:

- an RFC 3339 timestamp;
- the fixed level `error`;
- a bounded machine-readable category;
- a fixed content-free message.

Logs therefore do not contain article text, annotation or journal content,
settings, URLs, filesystem paths, access tokens, passwords, or API keys. Logs
live under the application data directory with private permissions where the
platform supports them. Each file is limited to 512 KiB and four rotated files
are retained. Diagnostic logs are not part of normal or portable backups.

## Diagnostic archive

The ZIP format version is `1` and has an allowlisted layout:

```text
diagnostics.json
errors.jsonl
README.txt
```

No database, document, attachment, media, search-index, settings, or raw log
file is copied into the archive. The structured report is generated from the
same privacy-safe values shown on the Status page.

The archive is written through the shared atomic-file contract: a sibling
temporary file is completed, synchronized, reopened, checked for the exact
three-entry layout, and parsed before it replaces the selected destination.
Export into the application data directory is refused to prevent accidental
replacement of live data.
