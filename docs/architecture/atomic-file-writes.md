# Atomic file-write contract

SoheiDesk must not damage an existing user export when a write is interrupted,
validation fails, or the destination filesystem runs out of space.

The contract applies to all current user-facing file outputs:

- journal and annotation Markdown;
- formatted Markdown, HTML, Typst, and LaTeX;
- period reports;
- DOCX;
- bibliography exports;
- portable JSON templates.

## Write sequence

1. Create a unique hidden temporary file in the destination directory with
   exclusive creation enabled.
2. Write the complete payload without modifying the current destination.
3. Flush file contents and metadata with `sync_all`.
4. Reopen or inspect the temporary output and verify it. Text and JSON exports
   are compared byte-for-byte with the generated payload. DOCX exports are
   reopened as ZIP archives and checked for their required entries and document
   body.
5. Replace the destination with one same-directory operation. Unix uses
   `rename`; Windows uses `MoveFileExW` with replace and write-through flags.
6. Synchronize the parent directory where the platform exposes directory
   synchronization.

Temporary files are removed on every failure before replacement. If the
destination already exists, its permissions are preserved. Symbolic-link and
non-regular-file destinations are rejected rather than replaced ambiguously.

## Failure behavior

- A write or validation error leaves the previous destination unchanged.
- A failed final replacement leaves the previous destination unchanged and
  removes the temporary file.
- A new destination is not made visible until its complete contents have been
  written and validated.
- Temporary files use the same directory as the destination, avoiding a
  cross-filesystem move that could degrade into copy-and-delete behavior.

The shared implementation lives in `src-tauri/src/atomic_file.rs`. New
user-facing file writers must use it and add a format-specific validator when a
byte-for-byte check is insufficient.
