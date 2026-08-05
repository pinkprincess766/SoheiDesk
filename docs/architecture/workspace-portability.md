# Portable workspace format

SoheiDesk portable workspaces are standard ZIP archives. The current format
version is `1`. They are intended for long-term access and migration, not for
the automatic retention policy used by recovery backups.

## Archive layout

```text
soheidesk-backup/
├── manifest.json
├── database.sqlite
├── attachments/
├── media/
└── README.txt
```

- `database.sqlite` is a consistent snapshot made with the SQLite Online
  Backup API; the live WAL-backed file is never copied directly.
- `attachments/` contains available source documents and journal fields whose
  template type is `file`.
- `media/` contains app-managed extracted images and cached media. The
  rebuildable search index is intentionally excluded.
- `README.txt` explains how to inspect the package without SoheiDesk.
- `manifest.json` records the format, app and database schema versions, record
  counts, missing external references, path rewrites, and a size plus SHA-256
  digest for every payload file.

SQLite and the payload files remain usable with ordinary tools. The package is
not encrypted.

## Import contract

Import is intentionally two-step:

1. Preview checks the ZIP structure, rejects unsafe or duplicate paths, streams
   every payload through its declared size and SHA-256, runs SQLite
   `integrity_check`, verifies schema compatibility and record counts, and
   displays the package contents.
2. Import requires the preview token and rechecks that neither the archive nor
   the current database changed. A non-empty workspace requires an explicit
   replacement confirmation.
3. SoheiDesk creates a verified emergency backup before changing data.
4. Copied attachment paths are rewritten only inside the imported staging
   database to point at app-managed `attachments/`; original external files are
   never overwritten.
5. Database, attachments, and media are installed with rollback support. The
   search index is rebuilt after success.

An older supported schema is migrated through the normal transactional
migration contract. A package with a newer schema or unknown format is rejected
with a compatibility error and leaves current data untouched.
