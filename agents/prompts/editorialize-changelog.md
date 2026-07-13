Write an editorialized replacement for only the newest release section in `CHANGELOG.md`.

The newest release section begins at the first release heading (`## `) and ends immediately before
the next release heading. Read `CHANGELOG.md` and the local Git history, but do not edit any file
and do not use the network. Compare the new section with several preceding releases and match their
established section names, ordering, tone, and Markdown style. Inspect the included local changes
when a generated title is not enough to classify or describe an entry accurately.

Apply these rules:

- Preserve the release version and date.
- For every retained entry, preserve its pull request number and exact URL. Never modify a URL.
- Drop entries that are clearly internal-only and have no user-facing effect, including CI or test
  runner changes, repository reorganization, and agent or developer infrastructure. If the effect is
  uncertain, keep the entry.
- Existing placement under `Enhancements` or `Bug fixes` is repository metadata. Never move an entry
  between those two sections. Do not move an entry from `Bug fixes` to `Performance`.
- Apply a feature-area override only when it is unambiguous: keep any change to a preview feature
  under `Preview features`, even when it fixes a bug; use `Python` for Python runtime or
  distribution changes; and move an entry from `Enhancements` or `Other changes` to `Performance`
  only when performance is the primary intent of the local change. Move an entry from
  `Other changes` to a more specific section only when the local change unambiguously fits one. Keep
  retained entries in `Other changes` when no established section fits.
- Treat the generated wording as source material, not a preferred baseline. Rewrite retained entries
  to make them clearer, more precise, and more user-facing. Expand internal shorthand and add
  missing context when supported by the local changes. Preserve the original meaning and do not
  invent or broaden claims. Avoid purely stylistic synonym changes.
- Preserve the relative order of entries already in each section. When moving entries from
  `Other changes`, append them to the destination section in their original order. Remove empty
  sections.

Return only the complete replacement release section, beginning with its `## ` heading. Do not
include the next release heading, any older changelog content, a code fence, or commentary. Your
response must contain exactly one line that begins with `## `; `### ` subsection headings are
expected.
