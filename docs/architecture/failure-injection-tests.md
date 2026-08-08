# Failure-recovery test contract

SoheiDesk treats recovery behavior as an executable contract. The test suite
uses deterministic fault-injection points where a real process kill, full disk,
or hardware failure would otherwise make the result slow or nondeterministic.
The injected failure is placed at the same transaction or atomic-write boundary
used by production code.

## Scenario matrix

| Scenario | Automated test | Required invariant |
| --- | --- | --- |
| Crash during autosave | `journal::tests::simulated_autosave_crash_before_commit_preserves_previous_draft` | An interruption after the upsert but before commit rolls back and preserves the last complete draft. |
| Two near-simultaneous saves | `journal::tests::two_near_simultaneous_autosaves_leave_one_complete_payload` | Both writes complete without corruption; the stored JSON is exactly one complete submitted payload. |
| Corrupted database | `db::tests::corrupted_database_is_rejected_without_replacing_its_bytes` | Startup rejects the database before WAL configuration and leaves its forensic bytes unchanged. |
| Interrupted migration | `db::migrations::tests::failed_sql_rolls_back_the_whole_migration`, `failed_commit_is_explicitly_rolled_back`, and `failed_post_migration_check_rolls_back_schema_and_version` | Schema objects, data, and the version marker remain on the last verified version. |
| Backup restore | `backup::tests::restore_round_trip_creates_emergency_copy_and_restores_media` and `restore_checks_integrity_before_touching_current_data` | A valid restore includes database, attachments, and media plus an emergency copy; a damaged backup cannot touch current data. |
| Full or unavailable disk | `atomic_file::tests::simulated_full_disk_preserves_existing_file` | A partial sibling write is removed and the existing destination remains byte-for-byte intact. |
| Moved and changed PDF | `library::tests::moved_document_keeps_identity_annotations_and_history` and `changed_document_rebinds_or_flags_annotations_without_deleting_them` | Movement preserves identity; changed content creates history and keeps every annotation either rebound or marked for review. |
| Lost attachment | `portability::tests::lost_attachment_is_reported_without_aborting_workspace_export` | The workspace remains exportable, while every missing reference is explicit in both the result and manifest. |
| Corrupted imported archive | `portability::tests::corrupted_import_archive_is_rejected_during_preview_without_touching_current_data` | Validation fails before authorization, emergency backup, or any target mutation. |
| Save while closing the window | `tests/draft-lifecycle.test.ts` — `dirty window waits for both draft saves before closing` and `failed draft save keeps the closing window open` | Close is prevented until both stores succeed; any failed save leaves the window open. |
| Recovery after forced termination | `journal::tests::committed_draft_is_recoverable_after_forced_termination_reopen` and `tests/draft-lifecycle.test.ts` — `forced-termination drafts are retained conservatively` | A committed file-backed draft survives a fresh database process, and uncertain or orphaned drafts are retained for review. |

## Production boundaries exercised

- Journal draft writes use `BEGIN IMMEDIATE`, a whole-payload upsert, and one
  commit. The autosave fault hook runs inside that real transaction.
- Window-close tests call the same lifecycle helper used by `JournalView.vue`.
- Database and draft-recovery tests use file-backed SQLite databases, not only
  in-memory connections.
- Workspace and backup tests validate ordinary ZIP files with the production
  checksum, preview, emergency-copy, and atomic-replacement code paths.
- Document identity tests open real temporary files and verify the library,
  version, and annotation records together.

These tests do not claim to emulate operating-system or hardware faults in all
possible environments. Installer-level kill tests, real disk exhaustion, and
platform UI automation remain release-validation tasks; the deterministic suite
guards the data-integrity boundaries on every CI run.

## Local commands

```bash
cargo test --manifest-path src-tauri/Cargo.toml
node --experimental-strip-types --test tests/*.test.ts
```
