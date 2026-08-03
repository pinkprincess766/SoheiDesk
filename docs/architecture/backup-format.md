# SoheiDesk backup format

SoheiDesk backups are ZIP archives stored in the `backups` directory under the
application data directory. The current format version is `1`.

## Archive layout

```text
manifest.json
database/soheidesk.sqlite
media/...
user-data/settings.json
user-data/templates.json
```

- `database/soheidesk.sqlite` is produced with the SQLite Online Backup API.
  The live database file is never copied directly while WAL mode is active.
- `media/` contains files managed by SoheiDesk, including cached document
  copies, extracted images, and app-managed attachments.
- `user-data/settings.json` and `user-data/templates.json` are readable exports
  from the same database snapshot. The SQLite database remains the canonical
  source used during restore.
- `manifest.json` records the format version, backup ID and kind, creation time,
  application and schema versions, and the size and SHA-256 digest of every
  payload file.
- `tantivy_index/` is not included because it can be rebuilt from the restored
  database and documents.

## Restore safety

Before changing current data, SoheiDesk:

1. Rejects unknown format or future schema versions and unsafe archive paths.
2. Checks the declared size and SHA-256 digest of every payload file.
3. Opens the extracted database read-only and runs `PRAGMA integrity_check(1)`.
4. Creates and verifies an emergency backup of the current state.

The database is restored through SQLite's backup API. App-managed media is
swapped with rollback support. If the database restore or a required migration
fails, SoheiDesk restores the emergency database and previous media. The search
index is rebuilt only after the data restore succeeds.

## Retention

Automatic daily archives use a `7 daily + 4 older weekly` policy. Manual,
pre-migration, and emergency backups are not removed by automatic retention.

Backup archives are private local files (`0600` on Unix), but they are not
encrypted. Use operating-system disk encryption and protect any copied archive
as you would protect the original research data.
