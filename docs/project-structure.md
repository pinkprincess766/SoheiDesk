# Project structure

SoheiDesk keeps the repository root limited to standard entry points used by
pnpm, Cargo, GitHub, and readers of the project. Product code and supporting
material belong to explicit directories.

```text
SoheiDesk/
|- frontend/            Vue source, Vite config, and public web assets
|- src-tauri/            Rust core, Tauri config, capabilities, and icons
|- tests/
|  |- fixtures/          Documents used only by automated tests
|- resources/
|  |- brand/             Source artwork for generated application icons
|- docs/
|  |- architecture/      Data formats and reliability contracts
|  |- images/            README screenshots and demonstrations
|  |- releases/          Version-specific release notes
|  |- translations/      Translated project overviews
|- scripts/              Local maintenance and build helpers
|- .github/workflows/    CI and release automation
|- package.json          Frontend and Tauri command entry point
|- deny.toml             Rust dependency policy
|- README.md             English project overview
`- LICENSE               Project license
```

## Placement rules

- Keep runtime Vue code under `frontend/src/`.
- Keep Rust modules under `src-tauri/src/` and expose commands through the
  existing command modules instead of adding Rust files at the repository root.
- Put reproducible test inputs under `tests/fixtures/`; application code must
  not depend on them.
- Put internal contracts and persistent-format documentation under
  `docs/architecture/`.
- Put translated project documentation under `docs/translations/` and link it
  from the root English README.
- Keep generated output out of Git. Frontend builds belong in `frontend/dist/`
  and Rust builds belong in `src-tauri/target/`.
- Add a new top-level directory only when none of the existing ownership
  boundaries describe the content.

The layout changes file ownership only. It does not change database locations,
backup contents, migration behavior, or user-facing file formats.

## Why some files stay in the root

The remaining root files are conventional discovery points rather than loose
project content. Git applies `.gitignore` and `.gitattributes` from the root to
the whole repository. GitHub discovers `README.md` and `LICENSE` there, pnpm
expects its package, lock, and workspace files there, and `cargo-deny` discovers
`deny.toml` there without an extra command-line path. Moving these files would
make the repository less conventional and the build commands more fragile.
