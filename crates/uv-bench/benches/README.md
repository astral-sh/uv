# Benchmarks

Run the commands below from the repository root.

## Git sources

`git_fetch` measures cold and cached fetches of pinned Python repositories and successive Django
releases through the local Git transport. The Rust fixtures download and verify the pinned histories
before measurement, then retain them in `target/bench-git` for subsequent runs.

```console
uv run --only-dev cargo codspeed build -m walltime --profile profiling -p uv-bench --bench git_fetch
uv run --only-dev cargo codspeed run -m walltime -p uv-bench --bench git_fetch git_fetch/
```
