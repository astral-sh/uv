# Build dependency cases from the uv issue tracker

The highest-risk case is **a second resolution after a backend runs**. In
[#17950](https://github.com/astral-sh/uv/issues/17950), the first build environment for CLIP
installed `setuptools==69.5.1`. The backend then requested `wheel`. The second resolution
installed `wheel` and `packaging` **and replaced setuptools with 82.0.0**, breaking the build.
The reporter confirmed that a build constraint fixed the original problem. For locked installs,
the stronger invariant is that a hook cannot cause uv to install anything outside the audited
build graph, even if the new requirement is itself harmless. Test with an empty wheel cache: a
cached wheel can bypass the hook entirely.

| Case | What the issue demonstrates | What to check |
| --- | --- | --- |
| [Source-only build cycle](https://github.com/astral-sh/uv/issues/10255) | Building `foo` requires `bar`; even the wheel for `bar` requires `foo` at runtime. The reproducer disables wheels for `foo`. | Recording a cycle in the graph must terminate. Actually building a source-only cycle must fail with a dependency chain, without waiting on a build already in progress. |
| [Index-specific build dependency](https://github.com/astral-sh/uv/issues/18059#issuecomment-3915122087) | A `uv pip` reproducer selects `torch==2.10.0+cu128` from an explicit index. Matching its version for the `flash-attn` build is insufficient: that distribution is absent from PyPI. | Lock the source as well as the version selected by `match-runtime`. Test two explicit indexes with the same name and version but different artifact hashes. |
| [Workspace backend with a workspace dependency](https://github.com/astral-sh/uv/issues/19074) | `my-tool` builds with local `my-backend`, which depends on local `my-util`. The report includes a failure recognizing the transitive file URL. | Preserve the workspace source through both edges. A same-named registry package must never stand in for the local utility. |
| [Different Cython requirements per build](https://github.com/astral-sh/uv/issues/10233#issuecomment-2569097723) | The reporter wanted to constrain Cython for an older `gevent` without forcing an alpha Cython onto unrelated packages. The second incompatible consumer is hypothetical in this report. | Use two local consumers with disjoint Cython ranges. Both build environments should work, and neither should install Cython into the application. |
| [Backend caches and ABI changes](https://github.com/astral-sh/uv/issues/16850) | In a non-isolated build, changing torch from 2.6 to 2.7 reportedly left a stale compiled extension even with separate uv wheel-cache entries. Removing the backend's intermediate files helped. | Distinguish auditing downloaded inputs from reproducing previously compiled binaries. A passing warm-cache sync proves neither which backend would run nor that the binary matches the current ABI. |

**Scope.** This is a targeted search of issue reports about build constraints, build isolation,
cycles, and extra build dependencies. These are historical reports and proposed checks, not claims
that current uv reproduces the bugs or that the checks passed. No package code was downloaded or
executed for this issue review. Issue frequency is not an estimate of ecosystem prevalence.
