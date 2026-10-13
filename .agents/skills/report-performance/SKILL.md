---
name: report-performance
description:
  Plan, run, assess, and write performance evidence for pull requests in uv. Use when Codex
  benchmarks a change, investigates a performance regression, compares revisions or binaries, writes
  or reviews a pull request performance claim, summarizes Hyperfine, Criterion, or CodSpeed results,
  or evaluates runtime, throughput, memory, allocations, binary size, build time, or CI time.
---

# Report Performance

Produce concise performance claims that are scoped, reproducible, and honest about uncertainty.
Follow repository-specific benchmarking instructions and use the existing harness when one exists.

## Select a Mode

- **Plan and run:** Design and execute a comparison, then report it. Before reading results, record
  the expected effect, primary workload and metric, smallest meaningful change, cases to run, sample
  count or stopping rule, and permitted exclusions.
- **Audit:** Review existing evidence for equivalent work, comparable builds, calculations,
  uncertainty, scope, missing data, and tradeoffs. Report which choices were predetermined and flag
  missing protocol or post-hoc choices. Distinguish calculations you verified from measurements you
  did not reproduce; do not invent preregistration or silently rerun benchmarks.
- **Report supplied results:** Calculate and write from the provided artifacts. Label inputs and
  claims that were not independently verified.

Respect the requested mode. Reviewing results does not authorize builds, benchmark runs, or pull
request updates.

## Run a Comparison

1. Read the repository guidance and nearby benchmarks. Prefer their harness, fixtures, profiles,
   warmups, sampling model, and output format.
2. Choose a workload that exercises the changed path. Use an end-to-end workload when practical; use
   a focused benchmark to explain the mechanism. Pair pathological cases with ordinary ones before
   making broad claims.
3. Confirm that the compared variants perform equivalent work. Compare exit status, output,
   diagnostics, generated artifacts, and relevant side effects. Disclose intentional differences.
4. Identify the exact revisions, binaries, and build commands. Keep toolchain, dependencies,
   features, profile, and build procedure identical except for the intended change.
5. Define the measured state, such as cold or warm caches, and prevent one variant from warming the
   other. Use the harness's sampling and warmup model. Interleave their execution order when drift
   could matter and the harness does not already control ordering.
6. Preserve raw output and account for every planned and executed case, including neutral results,
   regressions, failures, and exclusions. Add or retain a regression benchmark when practical.

## Document the Evidence

When running a comparison, record:

- exact revisions, binaries, build profile, and build commands;
- benchmark command, fixture revision or input size, options, cache state, and material harness
  configuration;
- host, operating system, and architecture;
- attempted and accepted run counts, failures, and exclusions.

When auditing or reporting supplied results, include the available details and flag material
omissions rather than inventing them.

Include other controls, such as power mode, CPU affinity, background load, or network replay, only
when they were used or could materially affect the result. The report itself may omit immaterial
setup details as long as the retained artifacts make the comparison reproducible.

Use profiles or relevant counters when they help explain the result, but do not present correlation
as proof. For CI changes, distinguish queue time, elapsed workflow time, job or step duration, and
summed runner time. Compare equivalent runner types, cache states, and concurrency.

## Interpret the Result

- Lead with absolute values and a directional change: for example, "wall time decreased from 412 ms
  on main to 361 ms on the branch (12.4% lower)."
- For a lower-is-better metric, calculate `(baseline - comparison) / baseline`. For a
  higher-is-better metric, calculate `(comparison - baseline) / baseline`. Do not calculate a
  relative change from a zero or meaningless denominator.
- Prefer uncertainty for the change when the harness reports it; do not treat separate variation for
  each variant as uncertainty for their difference. Otherwise present repeated results and their
  variability descriptively. If runs were paired, analyze the pairs together. Label a single run as
  anecdotal.
- Keep workloads separate. Aggregate them only when the total or weighting has clear practical
  meaning, and explain that meaning. Do not arithmetic-average percentage changes across unrelated
  workloads; use weighted absolute totals for total-cost claims or a geometric mean of ratios for a
  normalized suite.
- Scope the claim to what was measured. Structural measurements such as allocations, type size, or
  profile share are supporting evidence unless an end-to-end effect was also measured.
- Report regressions and tradeoffs beside the improvement, including relevant changes in memory, CPU
  time, cache size, binary size, build time, or CI cost.

Never convert a timeout or OOM into a speedup factor unless the protocol provides valid bounds for
both values. If main exceeds 60 seconds and the branch completes in 29 ms, report those facts
separately; their quotient is not an observed or conservative speedup bound. For OOM, report a
memory bound only when the enforced limit is known.

## Write the Report

Lead with the result and material tradeoffs. For one workload, prefer two to four sentences followed
by the exact command:

````markdown
## Performance

On `<workload and state>`, `<statistic and metric>` changed from `<main result>` on main
(`<revision>`) to `<branch result>` on the branch (`<revision>`), `<directional change>`. Across
`<run count>`, `<brief uncertainty or variability statement>`.
`<Correctness check and material tradeoff.>`

```console
$ <build or preparation command, when needed>
$ <benchmark command>
```
````

For several workloads, use a compact table. When comparing a pull request to main, prefer `main` and
`branch` over "This PR"; otherwise use labels that identify the actual revisions or binaries:

```markdown
| Benchmark |      main |    branch |                 Change |
| --------- | --------: | --------: | ---------------------: |
| `<case>`  | `<value>` | `<value>` | `<directional change>` |
```

Put setup details and long Hyperfine, Criterion, profiler, or CodSpeed output after the summary or
in a collapsible section. Read [Useful Report Patterns](references/useful-report-patterns.md) when
choosing how to present scaling, uncertainty, attribution, or tradeoffs.

## Final Check

- The compared variants performed equivalent work, or the difference is disclosed.
- The claim names the workload, metric, absolute results, direction, and relevant uncertainty.
- The commands, revisions, state, and run counts are reproducible.
- Neutral results, regressions, failures, exclusions, scope limits, and tradeoffs are visible.
