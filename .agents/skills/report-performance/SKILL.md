---
name: report-performance
description:
  Plan, run, assess, and write performance evidence for pull requests in uv. Use when Codex
  benchmarks a change, investigates a performance regression, compares baseline and candidate
  builds, writes or reviews a pull request performance claim, summarizes Hyperfine, Criterion, or
  CodSpeed results, or evaluates runtime, throughput, memory, allocations, binary size, build time,
  or CI time.
---

# Report Performance

Produce performance claims that are scoped, reproducible, statistically honest, and useful to
reviewers. Follow repository-specific benchmarking instructions before applying this workflow.

Never convert a timeout or OOM outcome into a speedup factor unless the measurement protocol
provides valid bounds for both operands. Reject a requested factor that does not meet this rule.

## Select a Mode

- **Plan and run:** Design and execute a comparison, then report it.
- **Audit:** Review existing evidence for correctness, comparability, calculations, uncertainty,
  scope, and missing data. Do not silently rerun benchmarks.
- **Report supplied results:** Calculate and write from the provided artifacts. Clearly label inputs
  that were not independently verified.

Respect the requested mode. Reviewing or summarizing results does not by itself authorize builds,
benchmark runs, or pull request updates.

## Workflow

1. Read the repository guidance and nearby benchmark implementations. Prefer the existing harness,
   fixtures, build profiles, and result format.
2. Before inspecting results, define the hypothesis, primary workload and metric, metric direction,
   minimum meaningful effect, workload matrix, sample count or stopping rule, and exclusion policy.
3. Choose a representative workload. Use a real end-to-end workload when practical and a focused
   microbenchmark to isolate the changed path. Pair pathological cases with ordinary workloads
   before making user-facing claims.
4. Verify equivalent work before timing. Compare exit status, output, diagnostics, generated
   artifacts, relevant side effects, and completed work. Record intentional semantic differences.
5. Build and identify the baseline and candidate precisely. Keep the toolchain, dependencies,
   profile, features, and build procedure identical except for the intended change. Record commits,
   build commands, and executable paths, and confirm that each benchmark invokes the intended
   binary.
6. Control measurement state. Define cold, warm, or hot state and reset it independently before
   every observation. Prevent cross-variant contamination of application, filesystem, build, DNS,
   and network caches. Freeze or replay network inputs when feasible.
7. Repeat measurements enough to estimate uncertainty when feasible. Use paired randomized,
   alternating, or counterbalanced baseline/candidate order by default on the same host. Label an
   unavoidable single-run result as anecdotal.
8. Run the predefined matrix and stopping rule. Preserve raw output and report all planned and
   executed cases, including failures, neutral results, regressions, and justified exclusions. Add
   or retain a regression benchmark when practical.
9. Interpret the result conservatively. Separate measured outcomes from attribution evidence and
   disclose uncertainty, scope limits, alternative explanations, and tradeoffs.
10. Draft the evidence in the pull request description or test plan. Do not post, comment, or update
    a pull request unless the user explicitly asks.

## Choose Evidence

- Scope claims to the measured workload. Do not turn a rule-level or parser microbenchmark into a
  product-wide claim.
- Explain why the workload represents the changed path. Include fixture revisions, input sizes,
  command options, and cache state.
- Record the host, operating system, architecture, power mode, compiler, build profile, benchmark
  order, relevant background load, and CPU-affinity policy. Explain uncontrolled differences.
- Include warmups, attempted and accepted sample counts, failures, exclusions, and the predefined
  exclusion rule.
- Summarize key results in text or a table. Treat screenshots and transient CodSpeed links as
  supporting evidence, not the only record.
- Use profiles or mechanism-relevant counters when they strengthen attribution. File reads,
  allocations, graph nodes, syscalls, cache entries, and time in the changed span are useful, but
  they are not proof of causality unless a controlled intervention isolates the mechanism.
- For CI, distinguish queue time, critical-path latency, job or step duration, and summed runner
  time. Compare retained runs with equivalent runner types, cache states, and workflow concurrency;
  do not infer an improvement from a single before-and-after run.

## Interpret Results

- Report the absolute baseline and candidate values when both complete and are comparable.
- Report metric direction explicitly, such as "31.7% lower wall time", "18% lower peak RSS", or "12%
  higher throughput".
- For a lower-is-better metric, compute the reduction as `(baseline - candidate) / baseline`.
- For a higher-is-better metric, compute the increase as `(candidate - baseline) / baseline`.
- Compute a relative change only when the baseline is nonzero and meaningful; otherwise report the
  absolute change.
- For a lower-is-better factor, prefer "the baseline took 1.46x as long" over the ambiguous "1.46x
  faster".
- If the baseline exceeds a timeout, report `baseline > T` directly. Do not divide the timeout by a
  candidate mean, median, point estimate, or observed sample maximum. For example, if the baseline
  exceeds 60 seconds and the candidate median is 29 ms, report those two facts without a `>2,000x`
  factor. Derive a ratio lower bound only when the measurement protocol provides a defensible
  candidate upper bound. For OOM, report a memory bound only when the enforced limit is known.

  **Hard rule:** A timeout plus finite candidate samples supports two separate statements, not a
  factor. Never call their quotient an observed, conservative, or timeout-derived speedup bound. If
  a supplied draft or user request contains such a factor, remove it, state that the bound is
  unsupported, and report only the timeout and candidate measurements.

- For repeated measurements, use the harness-defined or preselected estimator consistently. Report
  sample count, an appropriate dispersion measure, and uncertainty for the baseline-to-candidate
  difference or ratio. Analyze paired runs as pairs.
- Call a comparison inconclusive when its effect interval includes the null: `0` for a difference or
  percent change and `1` for a ratio. Do not infer significance from overlap between separate
  baseline and candidate intervals. Without effect-level uncertainty, describe the comparison as
  descriptive rather than statistically conclusive.
- Keep per-workload results. Never arithmetic-average percentages across heterogeneous workloads.
  Use weighted totals for total cost or a geometric mean of ratios for a normalized suite, and
  explain the weighting and aggregation.
- Label structural measurements such as type size, allocation count, or profile share as proxies
  unless an end-to-end effect was also measured.
- Report tradeoffs in the same summary. Consider wall time, CPU time, memory, allocations, cache
  size, binary size, build time, and CI cost.

## Write the Report

Use any clear section in the pull request description. The following template is optional:

````markdown
## Performance

**Claim.** On `<workload and state>`, `<metric>` changed from `<baseline result>` to
`<candidate result>` (`<directional relative change or bound>`).

**Method.** Compared `<baseline revision and binary>` with `<candidate revision and binary>` using
`<environment and build details>`. Used `<state reset>`, `<order strategy>`, `<warmups>`, and
`<attempted/accepted runs and exclusion rule>`.

| Workload | Metric   |                 Baseline |                  This PR |                  Change |
| -------- | -------- | -----------------------: | -----------------------: | ----------------------: |
| `<case>` | `<name>` | `<value and dispersion>` | `<value and dispersion>` | `<effect and interval>` |

`<State whether lower or higher is better.>` Values are `<preselected statistic>` across `<runs>`.

**Attribution evidence.**
`<State the hypothesized mechanism and supporting profiles, counters, or controlled comparisons. Identify remaining alternative explanations.>`

**Caveats.** `<Neutral or regressing cases, uncertainty, scope limits, and tradeoffs.>`

**Reproduction.**

```console
$ <build baseline and candidate>
$ <prepare fixture and environment>
$ <reset state>
$ <run benchmark>
$ <analyze raw results>
```
````

Keep the summary compact. Put long Hyperfine, Criterion, profiler, or CodSpeed output after the
summary or in a collapsible section.

For examples of individual reporting techniques and their limitations, read
[Useful Report Patterns](references/useful-report-patterns.md).

## Final Check

Before presenting the report:

- Confirm baseline and candidate perform equivalent work or disclose the semantic difference.
- Confirm the claim names the measured workload and metric.
- Confirm the builds, binaries, environment, state reset, and execution order are reproducible.
- Confirm all planned and executed cases, failures, and exclusions are accounted for.
- Confirm the absolute result, relative result or bound, and metric direction agree.
- Confirm the sample count, dispersion, and effect-level uncertainty are included when applicable.
- Confirm a timeout or OOM result is not converted into a factor without a protocol-defined bound.
- Confirm microbenchmark results are not generalized beyond their scope.
- Confirm neutral cases, regressions, attribution limits, and tradeoffs are disclosed.
- Confirm another contributor can reproduce the comparison from the recorded setup.
