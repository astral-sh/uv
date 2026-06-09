---
name: report-performance
description:
  Plan, run, assess, and write performance evidence for pull requests in uv and similar Rust CLI
  projects. Use when Codex benchmarks a change, investigates a performance regression, compares
  baseline and candidate builds, writes or reviews a pull request performance claim, summarizes
  Hyperfine, Criterion, or CodSpeed results, or evaluates runtime, throughput, memory, allocations,
  binary size, build time, or CI time.
---

# Report Performance

Produce performance claims that are scoped, reproducible, statistically honest, and useful to
reviewers. Follow repository-specific benchmarking instructions before applying this workflow.

## Workflow

1. Read the repository guidance and nearby benchmark implementations. Prefer the existing benchmark
   harness, fixtures, build profiles, and result format.
2. Define the claim before benchmarking. Name the affected workflow, metric, cache or network state,
   input scale, and expected mechanism.
3. Choose a representative workload. Use a real end-to-end workload when practical and a focused
   microbenchmark when it helps isolate the changed path.
4. Identify the baseline and candidate precisely. Record revisions or binaries and keep build
   profiles, features, inputs, and material environment details comparable.
5. Design the comparison around the expected source of variance. Use warmups and repeated runs for
   short benchmarks. Interleave baseline and candidate runs when drift is likely. Test multiple
   scales, states, or platforms when the effect may vary across them.
6. Run the narrowest benchmark that tests the claim, then broaden only as needed. Preserve the raw
   output and add or retain a regression benchmark when practical.
7. Interpret the result conservatively. Separate measured outcomes from explanations and disclose
   neutral cases, regressions, uncertainty, attribution limits, and tradeoffs.
8. Draft the performance evidence in the pull request description or test plan. Do not post,
   comment, or update a pull request unless the user explicitly asks.

## Choose Evidence

- Scope claims to the measured workload. Do not turn a rule-level or parser microbenchmark into a
  product-wide claim.
- Explain why the workload represents the changed path. Include relevant fixture revisions, input
  sizes, command options, and cold, warm, or hot state.
- Include the benchmark command and required setup. Include the operating system, architecture,
  hardware, build profile, network controls, or background-load controls when they materially affect
  interpretation or reproduction.
- Pair pathological cases with an ordinary workload when practical. For complexity fixes, show how
  behavior changes across input size.
- Summarize key results in text or a table. Treat screenshots and transient CodSpeed links as
  supporting evidence, not the only record of the result.
- Use profiles or causal counters when they strengthen attribution. Useful counters include file
  reads, allocations, graph nodes, syscalls, cache entries, and time in the changed span.

## Interpret Results

- Report the absolute baseline and candidate values when both complete and are comparable.
- Report the metric direction explicitly, such as "31.7% lower wall time", "18% lower peak RSS", or
  "12% higher throughput".
- For a lower-is-better metric, compute the reduction as `(baseline - candidate) / baseline`.
- For a higher-is-better metric, compute the increase as `(candidate - baseline) / baseline`.
- If reporting a factor for a lower-is-better metric, prefer "the baseline took 1.46x as long" over
  the ambiguous "1.46x faster".
- If the baseline times out or exhausts memory, report the bound or outcome instead of inventing a
  finite ratio.
- For repeated measurements, report the mean or median, dispersion or confidence interval, and
  sample count when available.
- Describe results as marginal or inconclusive when their uncertainty includes no improvement.
- Label structural measurements such as type size, allocation count, or profile share as proxies
  unless an end-to-end effect was also measured.
- Report tradeoffs in the same summary. Consider wall time, CPU time, memory, allocations, cache
  size, binary size, build time, and CI cost.

## Write the Report

Use any clear section in the pull request description. The following template is optional:

````markdown
## Performance

**Claim.** On `<workload and state>`, `<metric>` changed from `<baseline result>` to
`<candidate result>` (`<directional relative change>`).

**Method.** Compared `<baseline revision>` with `<candidate revision>` using
`<relevant environment and build details>`. `<Cache state, warmups, and run count when applicable.>`

| Workload | Metric   |                  Baseline |                   This PR |                 Change |
| -------- | -------- | ------------------------: | ------------------------: | ---------------------: |
| `<case>` | `<name>` | `<value and uncertainty>` | `<value and uncertainty>` | `<directional change>` |

`<State whether lower or higher is better.>` Values are `<statistic>` across `<runs>` runs.

**Mechanism.** `<Why the change affects this workload; include a profile or counter if available.>`

**Caveats.** `<Neutral or regressing cases, noise, scope limits, and tradeoffs.>`

**Reproduction.**

```console
$ <benchmark command>
```
````

Keep the summary compact. Put long Hyperfine, Criterion, profiler, or CodSpeed output after the
summary or in a collapsible section.

## Exemplary Reports

- [uv #18311](https://github.com/astral-sh/uv/pull/18311): scope a whole-stack result correctly and
  connect it to a causal reduction in file reads.
- [uv #18094](https://github.com/astral-sh/uv/pull/18094): show scaling across input sizes and use a
  lower bound when the baseline times out.
- [uv #11894](https://github.com/astral-sh/uv/pull/11894): report an improvement on one platform and
  a neutral result on another.
- [Ruff #25266](https://github.com/astral-sh/ruff/pull/25266): give an exact microbenchmark command,
  absolute values, confidence intervals, and an unambiguous time reduction.
- [Ruff #20098](https://github.com/astral-sh/ruff/pull/20098): combine mechanism, synthetic scaling,
  a real pathological input, causal counters, and broader benchmarks.
- [Ruff #3439](https://github.com/astral-sh/ruff/pull/3439): compare alternatives using both wall
  time and peak memory before explaining the chosen tradeoff.
- [Ruff #21749](https://github.com/astral-sh/ruff/pull/21749): disclose a large memory improvement
  alongside a wall-time regression.
- [Ruff #15731](https://github.com/astral-sh/ruff/pull/15731): describe a small result as noisy when
  some runs are neutral or favor the baseline.

The [`performance` pull requests for uv][uv-performance-pulls] and [Ruff][ruff-performance-pulls]
provide more examples, but labels are not a complete archive.

[uv-performance-pulls]: https://github.com/astral-sh/uv/pulls?q=label%3Aperformance+is%3Amerged
[ruff-performance-pulls]: https://github.com/astral-sh/ruff/pulls?q=label%3Aperformance+is%3Amerged

## Final Check

Before presenting the report:

- Confirm the claim names the measured workload and metric.
- Confirm the baseline and candidate are identifiable and comparable.
- Confirm the absolute result, relative result or bound, and metric direction agree.
- Confirm uncertainty and sample count are included when applicable.
- Confirm microbenchmark results are not generalized beyond their scope.
- Confirm neutral cases, regressions, attribution limits, and tradeoffs are disclosed.
- Confirm another contributor can reproduce the comparison from the recorded setup.
