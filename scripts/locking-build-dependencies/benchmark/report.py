"""Render a standalone comparison from bench.py's results (including partial runs)."""

import json
import statistics
import sys
from bisect import bisect_right
from html import escape
from pathlib import Path


def histogram(values, title, unit):
    if not values:
        return ""
    # Fixed bands let successive runs be compared. Keep outliers in open-ended bins instead of
    # stretching the entire axis around one timeout or discarding the most expensive locks.
    cuts = (
        (-1, 0, 0.1, 0.5, 1, 2, 5, 15, 30)
        if unit == "seconds"
        else (-50, 0, 10, 25, 50, 100, 250, 500, 1000)
    )
    bins = [0] * (len(cuts) + 1)
    for value in values:
        bins[bisect_right(cuts, value)] += 1

    span = 360 / len(bins)
    marks = []
    for i, count in enumerate(bins):
        height = 100 * count / max(bins)
        if i == 0:
            label = f"below {cuts[0]}"
        elif i == len(cuts):
            label = f"{cuts[-1]} and above"
        else:
            label = f"from {cuts[i - 1]} up to {cuts[i]}"
        marks.append(
            f'<rect x="{i * span:.1f}" y="{102 - height:.1f}" width="{span - 2:.1f}" height="{height:.1f}">'
            f"<title>{label} {unit}: {count:,} pairs</title></rect>"
        )
        if i:
            marks.append(f'<text x="{i * span:.1f}" y="116">{cuts[i - 1]}</text>')

    return f"""
      <figure><figcaption>{title} ({unit})</figcaption>
      <svg role="img" aria-label="{title} for {len(values)} paired observations"
        viewBox="0 0 360 120">{"".join(marks)}</svg>
      <small>One bar per band; band widths differ. Tallest bar: {max(bins):,} pairs.</small></figure>"""


def offline_checks(rows):
    def cell(check):
        if check is None:
            return "not run"
        milliseconds = check["seconds"] * 1000
        if check["exit"] != 0:
            error = check.get("error") or f"Process exited with status {check['exit']}"
            return f"<details><summary>failed, {milliseconds:.0f} ms</summary><pre>{escape(error)}</pre></details>"
        return f"OK, {milliseconds:.0f} ms"

    checks = []
    for row in sorted(rows, key=lambda r: r["name"]):
        values = [row[side].get("offline_check") for side in ("baseline", "candidate")]
        if any(values):
            checks.append(
                f"<tr><th>{escape(row['name'])}</th><td>{cell(values[0])}</td><td>{cell(values[1])}</td></tr>"
            )
    if not checks:
        return ""
    return f"""
    <h2>Rechecking an unchanged lock without the network</h2>
    <p>Immediately after each successful lock, the same binary ran
    <code>uv lock --check --offline</code> on its own lockfile with its cache still populated.
    These checks are timed separately and excluded from the resolution chart above.</p>
    <div class="scroll"><table><thead><tr><th>Workload</th><th>Baseline</th><th>Candidate</th></tr></thead>
    <tbody>{"".join(checks)}</tbody></table></div>"""


def render(directory):
    meta = json.loads((directory / "method.json").read_text())
    with (directory / "results.jsonl").open() as stream:
        rows = list(map(json.loads, filter(str.strip, stream)))
    pairs = [r for r in rows if "added" in r]
    deltas = [r["candidate"]["seconds"] - r["baseline"]["seconds"] for r in pairs]
    relative = [
        100 * d / r["baseline"]["seconds"] for d, r in zip(deltas, pairs, strict=True)
    ]
    failed = [r for r in rows if "added" not in r]
    shown = sorted(
        pairs,
        key=lambda r: r["candidate"]["seconds"] - r["baseline"]["seconds"],
        reverse=True,
    )[:30]

    def table_row(r):
        a, b = r["baseline"], r["candidate"]
        links = ""
        for side in ("baseline", "candidate"):
            filename = f"{r['name']}-{side}.lock"
            if meta["workloads"] == "named" and (directory / filename).is_file():
                links += f' <a href="{escape(filename, quote=True)}">{side}</a>'
        return (
            f"<tr><th>{escape(r['name'])}{links}</th><td>{a['seconds']:.2f} → {b['seconds']:.2f}</td>"
            f"<td>{b['seconds'] - a['seconds']:+.2f}</td><td>{(b['seconds'] / a['seconds'] - 1) * 100:+.0f}%</td>"
            f"<td>{a['bytes']:,} → {b['bytes']:,}</td><td>+{r['added']} / −{r['removed']}</td>"
            f"<td>{a['edges']} → {b['edges']}</td></tr>"
        )

    baseline_failed = sum("error" in row["baseline"] for row in failed)
    candidate_failed = sum("error" in row["candidate"] for row in failed)
    details = []
    for r in sorted(failed, key=lambda r: "error" in r["baseline"])[:20]:
        errors = "".join(
            f"<b>{side.title()}</b><pre>{escape(r[side]['error'])}</pre>"
            for side in ("baseline", "candidate")
            if "error" in r[side]
        )
        details.append(
            f"<details><summary>{escape(r['name'])}</summary>{errors}</details>"
        )
    summary = (
        f"Median overhead: <strong>{statistics.median(deltas):+.2f} s "
        f"({statistics.median(relative):+.0f}%)</strong>."
        if pairs
        else "No paired timing is available."
    )
    page = f"""<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>What build dependency locking costs</title>
<style>
body{{margin:3rem auto;padding:0 1rem;max-width:72rem;font:16px/1.55 system-ui,sans-serif;color:#172635;background:#fcfcfa}}
h1,h2{{line-height:1.15}} h1{{font-size:clamp(2rem,5vw,3rem)}} p{{max-width:53rem}}
.charts{{display:flex;gap:2rem;flex-wrap:wrap}} figure{{margin:0;flex:1 1 18rem}} figcaption{{font-weight:bold}}
svg{{width:100%;fill:#288774;border-bottom:1px solid #7e918e}} svg text{{font:8px system-ui;fill:#172635;text-anchor:middle}}
.scroll{{overflow-x:auto}} table{{border-collapse:collapse;font-variant-numeric:tabular-nums}}
td,th{{padding:.55rem .75rem;border-bottom:1px solid #d1dad9;text-align:right;white-space:nowrap}}
th:first-child,summary{{text-align:left}} pre,code{{white-space:pre-wrap;overflow-wrap:anywhere}}
summary{{cursor:pointer}} details{{margin:.5rem 0}}
</style>
<main>
<h1>What build dependency locking costs</h1>
<p><b>{len(pairs):,} of {len(rows):,}</b> measured workloads locked successfully with both binaries
({meta["inputs"]:,} scheduled; {meta["jobs"]} concurrent pairs).
{summary} These single-run wall times include network traffic. {"Source builds were disabled." if meta["no_build"] else "Source builds were permitted only in isolated, unprivileged containers."}</p>
<div class="charts">{histogram(deltas, "Added lock time", "seconds")}{histogram(relative, "Relative change", "%")}</div>
{"<p><b>Run in progress.</b> Shorter locks can finish first; these partial distributions need not represent all scheduled workloads.</p>" if len(rows) < meta["inputs"] else ""}
<h2>Where the lock changed</h2>
<p>Showing {len(shown)} of {len(pairs):,} successful pairs, ordered by added seconds.
Package changes count distinct name, version and source combinations. Edges count explicit dependency
entries, so implicit, unambiguous build requirements do not inflate that number.</p>
<div class="scroll"><table><thead><tr><th>Workload</th><th>Seconds, base → build lock</th><th>Δ s</th><th>Δ %</th>
<th>Lockfile bytes</th><th>Packages +/−</th><th>Edges</th></tr></thead>
<tbody>{"".join(table_row(r) for r in shown)}</tbody></table></div>
{offline_checks(rows)}
<h2>Unpaired results ({len(failed):,})</h2>
<p>Only baseline succeeded: {len(failed) - baseline_failed:,}.
Only candidate succeeded: {len(failed) - candidate_failed:,}.
Both failed: {baseline_failed + candidate_failed - len(failed):,}.
<a href="results.jsonl">All raw observations</a> · <a href="method.json">Run parameters</a></p>
{"".join(details) or "<p>None.</p>"}{"<p>Showing 20 failures, prioritizing those where only the candidate failed.</p>" if len(failed) > 20 else ""}
<h2>How this was measured</h2>
<p>Workload: {escape(meta["workloads"])}. PyPI cutoff: <code>{escape(meta["cutoff"])}</code>.
Both sides use Python 3.11 and separate caches, initially empty and reused between projects.
The order alternates to reduce warm-up bias. Failures are excluded from time and size comparisons.</p>
<details><summary>Binary fingerprints (SHA-256)</summary><pre>{escape(json.dumps(meta["sha256"], indent=2))}</pre></details>
</main></html>"""
    (directory / "index.html").write_text(page)


if __name__ == "__main__":
    render(Path(sys.argv[1]))
