# Useful Report Patterns

These pull requests show useful reporting techniques rather than complete templates. Performance
labels are not complete archives; browse the merged [`uv` performance pull requests][uv-performance]
and [`Ruff` performance pull requests][ruff-performance] for more examples.

## Scope and Attribution

- [uv #18311](https://github.com/astral-sh/uv/pull/18311) scopes a whole-stack result to the
  measured workflow and uses file-read counts to support the proposed explanation.
- [Ruff #20098](https://github.com/astral-sh/ruff/pull/20098) combines synthetic scaling, a real
  pathological input, mechanism-relevant counters, and broader benchmarks.

## Scaling and Limits

- [uv #18094](https://github.com/astral-sh/uv/pull/18094) shows how behavior changes across input
  sizes and includes a timeout outcome. When one side times out, report the threshold and the
  completed measurement separately unless the protocol provides bounds for both values.

## Uncertainty

- [Ruff #25266](https://github.com/astral-sh/ruff/pull/25266) includes the exact benchmark command,
  absolute values, confidence intervals, and an unambiguous time reduction.
- [uv #11894](https://github.com/astral-sh/uv/pull/11894) shows results for multiple platforms,
  including a small change whose reported uncertainty includes no change, making the improvement
  inconclusive, and a neutral result elsewhere.
- [Ruff #15731](https://github.com/astral-sh/ruff/pull/15731) notes when a small local result varies
  across comparisons, including neutral runs and runs that favor main.

## Tradeoffs

- [Ruff #3439](https://github.com/astral-sh/ruff/pull/3439) compares wall time and peak memory, then
  chooses the lower-memory design when timing differences are inconclusive.
- [Ruff #21749](https://github.com/astral-sh/ruff/pull/21749) presents a large memory improvement
  alongside a wall-time regression.

[ruff-performance]: https://github.com/astral-sh/ruff/pulls?q=label%3Aperformance+is%3Amerged
[uv-performance]: https://github.com/astral-sh/uv/pulls?q=label%3Aperformance+is%3Amerged
