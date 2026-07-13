Rewrite the changelog entry for the release prepared in the previous step.

Read `CHANGELOG.md` and change only the newest release section added by the release commit at
`HEAD`. Compare it with the release sections that precede it and match their style. Inspect the
included changes in the local Git history when the generated title is not enough to write an
accurate, user-facing entry.

Preserve the release version, date, every included pull request, and every pull request link.
Improve the wording, categorization, and order of entries. Use concise imperative descriptions, the
established section names, and Markdown formatting consistent with prior releases. Put the most
significant user-facing changes first within each section and remove any empty sections.

Return only the complete rewritten contents of `CHANGELOG.md`, without a code fence or any other
commentary. Preserve all content outside the newest release section exactly.
