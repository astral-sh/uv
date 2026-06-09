# Useful Report Patterns

These pull requests illustrate individual reporting choices. They are not complete templates, and
some predate or intentionally omit parts of the workflow in this skill.

## uv

- [uv #18311](https://github.com/astral-sh/uv/pull/18311): scopes an aggregate whole-stack result
  and supports the proposed mechanism with file-read counters.
- [uv #18094](https://github.com/astral-sh/uv/pull/18094): shows scaling across input sizes and a
  timeout outcome. Its reported `>2,000x` factor is not a rigorous bound because the candidate value
  is a point estimate; prefer reporting `baseline > 60s` unless a defensible candidate upper bound
  is available.
- [uv #11894](https://github.com/astral-sh/uv/pull/11894): reports a faster ARM point estimate whose
  uncertainty includes no change, alongside a neutral x86 Windows result. Treat the ARM comparison
  as inconclusive rather than a demonstrated platform improvement.

The [`performance` pull requests for uv][uv-performance-pulls] provide more examples, but labels are
not a complete archive.

[uv-performance-pulls]: https://github.com/astral-sh/uv/pulls?q=label%3Aperformance+is%3Amerged

## Ruff

- [Ruff #25266](https://github.com/astral-sh/ruff/pull/25266): gives an exact microbenchmark
  command, absolute values, confidence intervals, and an unambiguous time reduction.
- [Ruff #20098](https://github.com/astral-sh/ruff/pull/20098): combines a proposed mechanism,
  synthetic scaling, a real pathological input, mechanism-relevant counters, and broader benchmarks.
- [Ruff #3439](https://github.com/astral-sh/ruff/pull/3439): compares alternatives using wall time
  and peak memory, then selects the lower-memory design when timing differences are inconclusive.
- [Ruff #21749](https://github.com/astral-sh/ruff/pull/21749): discloses a large memory improvement
  alongside a wall-time regression.
- [Ruff #15731](https://github.com/astral-sh/ruff/pull/15731): discloses that a small local result
  varies across comparisons, with some runs neutral or favoring the baseline.

The [`performance` pull requests for Ruff][ruff-performance-pulls] provide more examples, but labels
are not a complete archive.

[ruff-performance-pulls]: https://github.com/astral-sh/ruff/pulls?q=label%3Aperformance+is%3Amerged
