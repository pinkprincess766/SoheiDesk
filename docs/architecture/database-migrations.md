# Database migration contract

SoheiDesk stores its schema history in the `schema_migrations` table. The
current schema version is 4. Versions 0 through 3 are supported upgrade
sources; version 0 means a new, empty database with no application tables.

Startup follows this order:

1. Open the SQLite connection and inspect the complete migration history.
2. Refuse a newer, incomplete, gapped, or unversioned non-empty schema before
   applying persistent connection settings.
3. For an existing database, create and verify a pre-migration backup before
   every pending schema step.
4. Start an `IMMEDIATE` SQLite transaction.
5. Apply one migration and insert its version marker in the same transaction.
6. Run `PRAGMA quick_check(1)` before committing, so a failed check can still
   roll back the transaction.
7. Commit only when every operation succeeds. Otherwise, roll back the whole
   step and stop application startup.
8. Run `PRAGMA quick_check(1)` again against the committed database before the
   rest of the application starts.

A failed backup leaves the database untouched. A failed migration leaves the
database at its last complete version, so the backup and the original schema
remain available for diagnosis or recovery. Automatic downgrade is not
supported, and an older application will not open a database created by a
newer schema version.

## Adding a migration

- Append a new explicitly numbered entry to the migration catalog. Never
  rewrite a migration that may already exist in a released database.
- Keep transaction-control statements out of migration SQL; the migration
  runner owns the transaction boundary.
- Include schema changes and their version marker in the same migration step.
- Add assertions for the new schema objects and preserve upgrade coverage from
  every supported historical version.
- Add a failure-path test when the migration transforms or removes existing
  data.

Pre-migration archives use the same verified format described in
[backup-format.md](./backup-format.md).
