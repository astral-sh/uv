Find the requested number of confirmed, independently fixable bugs in uv that are worth fixing and
easy to review. Prioritize user-visible correctness, integrity, and safety defects over speculative
cleanup. Do not count variants of the same defect, known duplicates, or incomplete design work as
separate findings.

Read `CONTRIBUTING.md`, `AGENTS.md`, the relevant code and nearby tests, and local Git history.
Inspect linked specifications, issues, pull requests, and comparable implementations when they can
establish expected or ecosystem behavior. Pin the investigation to one reasonable upstream base; do
not repeatedly fetch, rebase, or chase a moving `main`. Do not edit files, publish changes, or
interact publicly unless requested. Treat source distributions and build backends as potentially
arbitrary code and use an isolated environment for unsafe reproductions.

Apply these rules when selecting findings:

- Demonstrate the defect. Identify the affected interface, minimal input or command, expected and
  actual behavior, precise production mechanism, and user impact. A suspicious code pattern, TODO,
  specification mismatch, or missing test is a lead, not a confirmed bug. Clearly separate facts
  from inference and state the confidence in each finding.
- Check that the behavior is actually unintended. Compare sibling paths such as build/install,
  lock/sync, pip/project/tool, editable/noneditable, and CLI/environment/config resolution. Check
  relevant pip, CPython, and PyPA behavior. Distinguish a specification **MUST** from a **SHOULD**,
  a recommendation, or an implementation convention; do not describe a recommendation as a security
  boundary.
- Search existing findings, open and closed issues, draft and merged pull requests, reverts, recent
  history, and nearby comments before selecting a candidate. Look for prior compatibility failures
  and active changes to the same production path, not just matching titles. Record the concrete
  overlap or prior decision. Do not propose duplicates, unsolicited features, or work labeled
  `needs-decision` or `needs-design` as ready-to-publish fixes.
- Treat every new rejection, normalization, identity, cache-key, lock-format, precedence, or
  warning-to-error change as compatibility-sensitive. Check realistic producers, published or legacy
  inputs, platform behavior, existing escapes, and migration paths. Choose deliberately between
  warning, rejection, healing, migration, or staged behavior. Do not rewrite a realistic malformed
  fixture merely to make a stricter implementation pass.
- Verify the whole safety invariant. Follow filesystem, archive, credential, redirect, cache, and
  immutable-identity boundaries through nested symlinks, traversal, alternate encodings, case and
  layout differences, and failure after mutation. User-authored local project configuration may be
  trusted for the claimed boundary; downloaded or transitive source distributions and their build
  backends remain arbitrary inputs. Explain the actual untrusted boundary before claiming a security
  issue and never claim protection broader than the demonstrated path.
- Keep the proposed fix narrow. Map every production hunk to the stated defect and split distinct
  semantics into separate findings. In particular, separate control-character safety from broader
  grammar compliance, format migration from behavior, transaction handling from cleanup, and
  refactoring or dependency changes from a correctness fix. A small reproducer does not justify a
  broad validator or rewrite.
- Describe a realistic regression path before accepting a finding. Cover the important matrix:
  platform, case sensitivity, symlinks and custom layouts; malformed and legacy inputs; empty,
  unset, quoted, and escaped values; private indexes and providers; precedence and escape hatches.
  For state changes, exercise the second invocation (relock, reinstall, uninstall, or rerun) and
  verify that failure leaves no partial state.

Classify each finding as `LOW`, `RISK`, `REWORK`, or `HOLD`. `RISK` means there is credible
ecosystem or compatibility impact, not that the defect is unimportant; a well-motivated
specification fix can still be valuable. Use `REWORK` for a valid defect whose proposed fix is too
broad or incomplete. Use `HOLD` for demonstrated breakage, an active duplicate, an unresolved policy
decision, or an incomplete safety invariant, and explain what evidence or decision would unblock it.
The requested number counts only confirmed, independently fixable `LOW` or `RISK` findings and
`REWORK` findings once corrected. Report held candidates and duplicates separately, unnumbered;
never publish them or quietly use them to fill the requested count.

If implementation is requested, apply these additional rules:

- Use one isolated branch and one discrete commit per independently reviewable fix. Preserve
  unrelated work and the pinned base. Before composing or publishing overlapping fixes, check that
  their shared production hunks and regression matrices agree.
- Attempt to add regression coverage for every changed behavior, preferring focused integration
  tests and nearby `insta` patterns. Run the narrow relevant tests, formatting, and
  `git diff --check`; use the repository's documented lint and platform checks where applicable,
  including `cargo xwin clippy` for Windows changes from Unix. Do not use a release profile unless
  requested or reproducing a performance issue, broadly update dependencies, or add snapshot filters
  to hide changed or nondeterministic behavior. Use `cargo update --precise` for necessary lockfile
  changes, capture deterministic snapshot values directly, and explain genuinely platform-dependent
  output.
- Perform a final self-review of the complete diff. Verify the production mechanism, regression,
  error and warning behavior, second invocation, compatibility escape, imports and naming, new
  dependencies, and repository conventions. Confirm each branch contains exactly its intended commit
  and that the recorded branch and commit identifiers are current.

If publication is explicitly requested, open drafts with concise maintainer-facing prose that leads
with the observed problem, explains the mechanism and narrow effect, and links relevant external
specification, issue, or prior-revert context. Use inline-code styling for commands, flags,
configuration keys, paths, and identifiers. Publish drafts directly to `astral-sh/uv`. Do not add a
pull request template, validation results, links that merely point to uv's own documentation,
automated-title prefixes, or co-author trailers. Never prefix a title with `[codex]` or apply the
`codex` label. Prefer one appropriate semantic label and add `breaking` only when behavior is
genuinely incompatible. Apply a security label only for a demonstrated untrusted filesystem,
credential, archive, cache, or immutable-identity boundary, not merely specification noncompliance
or malformed local configuration. Check for an existing pull request before creating another. Never
post a new public comment; draft the proposed text for the user instead. Do not post replies or
review-thread messages unless explicitly requested.

Return a numbered list of findings. For each finding, include a concise title and classification,
the observed versus expected behavior with a minimal reproduction, the production cause and relevant
paths, compatibility or ecosystem evidence with direct links where useful, the proposed narrow fix
and regression, and the confidence. End with an unnumbered list of duplicates, holds, and
cross-finding overlaps that require coordination. Do not invent evidence, overstate impact, or
report a requested count as complete until every included finding meets the same standard.
