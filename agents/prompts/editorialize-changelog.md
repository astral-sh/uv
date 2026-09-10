Write an editorialized replacement for only the newest release section in `CHANGELOG.md`.

The newest release section begins at the first release heading (`## `) and ends immediately before
the next release heading. Read `CHANGELOG.md` and the local Git history, but do not edit any file
and do not use the network. Compare the new section with several preceding releases and match their
established section names, ordering, tone, and Markdown style. Inspect the included local changes
when a generated title is not enough to classify or describe an entry accurately.

In any GitHub-facing output, write issue and pull request references in the canonical
owner/repository#number form, such as astral-sh/uv#123 or astral-sh/uv-dev#123. This preserves
cross-repository closing keywords and lets GitHub render the references as links. Do not use bare
numbers, repository-name shorthand, Markdown link syntax, or backticks around references. Preserve
the established formatting of references in `CHANGELOG.md`.

Apply these rules:

- Preserve the release version and date.
- For every retained entry, preserve its pull request number and exact URL. Never modify a URL.
- Keep each Markdown paragraph and list item on a single line, including its pull request links. Do
  not hard-wrap changelog prose; wrapping can break rendering on GitHub.
- Drop entries that are clearly internal-only and have no user-facing effect, including CI or test
  runner changes, repository reorganization, and agent or developer infrastructure. If the effect is
  uncertain, keep the entry.
- Existing placement under `Enhancements` or `Bug fixes` is repository metadata. Never move an entry
  between those two sections unless splitting distinct changes as described below. Do not move an
  entry from `Bug fixes` to `Performance`.
- Keep one independently useful user-facing change per bullet. Combine pull requests that implement
  the same change, preserving all references. Split a pull request that contains distinct changes
  into separate entries, even in different sections when each change unambiguously belongs there.
  Repeat the same pull request number and exact URL in each entry. Do not split a change into its
  implementation steps.
- Apply a feature-area override only when it is unambiguous: keep any change to a preview feature
  under `Preview features`, even when it fixes a bug; use `Python` for changes to available Python
  runtimes and managed distributions, not for general uv bugs that involve an interpreter or virtual
  environment; and move an entry from `Enhancements` or `Other changes` to `Performance` only when
  performance is the primary intent of the local change. Move an entry from `Other changes` to a
  more specific section only when the local change unambiguously fits one. Keep retained entries in
  `Other changes` when no established section fits and they describe user- or ecosystem-relevant
  maintenance, such as MSRV or toolchain updates and public API compatibility for downstream
  integrations.
- Treat the generated wording as source material, not a preferred baseline. Rewrite retained entries
  to make them clearer, more precise, and more user-facing. Expand internal shorthand and add
  missing context when supported by the local changes. Preserve the original meaning and do not
  invent or broaden claims. Avoid purely stylistic synonym changes.
- Lead with what users can do or what behavior is fixed. Keep entries concise, omitting incidental
  implementation details and exhaustive lists of affected flags or edge cases. Retain qualifiers
  needed to avoid overstating the change, such as opt-in behavior or affected platforms.
- For routine Python releases, prefer `Add CPython <version>` or the equivalent runtime name. Omit
  download-table and sysconfig implementation details. For managed-runtime rebuilds, identify the
  noteworthy dependency or security update, including versions when useful, instead of only naming
  the upstream build release. Keep changes to uv's Docker images in `Other changes` unless another
  section clearly fits.
- For an unusually significant release-wide change, use a short introductory paragraph after the
  release date to explain the user-facing effect and any action needed. Preserve the pull request
  references and avoid repeating the same announcement as a routine bullet.
- Put the most significant user-facing entries first within each section and remove empty sections.

Return only the complete replacement release section, beginning with its `## ` heading. Do not
include the next release heading, any older changelog content, a code fence, or commentary. Your
response must contain exactly one line that begins with `## `; `### ` subsection headings are
expected.
