- Treat placement under `Enhancements` and `Bug fixes` as repository metadata. Move entries between
  those two sections only when splitting distinct changes that clearly belong in different sections.
  Never move an entry from `Bug fixes` to `Performance`.
- Keep changes to preview features in `Preview features`, including bug fixes. Use `Python` for
  available runtimes and managed distributions. General uv bugs involving an interpreter or virtual
  environment belong in their usual sections.
- Move an entry from `Enhancements` or `Other changes` to `Performance` only when performance is the
  primary intent. Keep MSRV and toolchain updates, public API compatibility maintenance, and Docker
  image changes in `Other changes` when no more specific section clearly fits.
- For routine runtime additions, prefer `Add CPython <version>` or the equivalent runtime name. Omit
  download-table and sysconfig implementation details. For managed-runtime rebuilds, name the
  noteworthy dependency or security update and include versions when useful.
