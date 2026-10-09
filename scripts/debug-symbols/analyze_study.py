"""Registered, simultaneous median comparisons over independent runner blocks."""

import argparse
import hashlib
import itertools
import json
import math
import statistics
from pathlib import Path

LEVELS = ("none", "line-tables-only", "limited", "full")
METRICS = ("build", "jupyter", "trio")


def plan_fingerprint(plan):
    return hashlib.sha256(
        json.dumps(plan, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def median_interval(values, alpha):
    """Exact order-statistic interval under independent identically distributed blocks.

    Choose the widest binomial tail rejection set whose probability is at most
    alpha/2. Ties make coverage conservative. An unbounded interval is intentional
    when the sample cannot support the requested confidence level.
    """
    ordered = sorted(values)
    count = len(ordered)
    rank = 0
    tail = 0
    for candidate in range(1, count // 2 + 1):
        tail += math.comb(count, candidate - 1)
        if 2 * tail / (2**count) <= alpha:
            rank = candidate
        else:
            break
    if not rank:
        return -math.inf, math.inf
    return ordered[rank - 1], ordered[count - rank]


def log_value(report, treatment, metric):
    if metric == "build":
        builds = report["treatments"][treatment]["builds"]
        measured = next(iter(builds.values()))
        return math.log(measured["total_seconds"])
    return math.log(report["benchmarks"][metric]["median_seconds"][treatment])


def treatment_value(report, treatment, metric):
    if treatment == "none":
        return statistics.mean(
            log_value(report, name, metric) for name in ("none-a", "none-b")
        )
    return log_value(report, treatment, metric)


def classify(lower, upper, margin, *, control=False):
    equivalent = lower >= math.log1p(-margin) and upper <= math.log1p(margin)
    if equivalent:
        return "equivalent_within_margin"
    if not control and (lower > 0 or upper < 0):
        return "difference_detected"
    return "unresolved"


def analyze(reports, plan, sample_size):
    if (
        sample_size < 20
        or sample_size % 20
        or (sample_size // 20) & (sample_size // 20 - 1)
    ):
        raise ValueError(
            "Inferential looks are only permitted at20,40,80,... blocks per platform"
        )
    look_index = (sample_size // 20).bit_length() - 1
    look_alpha = plan["family_alpha"] / (2 ** (look_index + 1))
    interval_alpha = look_alpha / plan["simultaneous_contrasts"]
    grouped = {}
    identities = set()
    runners = set()
    commits = set()
    for report in reports:
        if report["phase"] != "confirmatory":
            continue
        if report["study"] != plan["study_id"] or not report["verified"]:
            raise ValueError("Invalid or incomplete study block")
        if report["plan_sha256"] != plan_fingerprint(plan):
            raise ValueError("Registered analysis plan changed")
        if report["source_fingerprints"] != plan["source_fingerprints"]:
            raise ValueError("Study inputs changed")
        identity = (report["target"], report["block"])
        if identity in identities:
            raise ValueError("A runner block was supplied more than once")
        identities.add(identity)
        runner = report["runner"]["RUNNER_NAME"]
        if not runner or (report["target"], runner) in runners:
            raise ValueError("Independent runner allocations are required")
        runners.add((report["target"], runner))
        commits.add(report["commit"])
        grouped.setdefault(report["target"], []).append(report)
    if len(commits) != 1 or set(grouped) != set(plan["targets"]):
        raise ValueError(
            "One frozen experiment commit and all four platforms are required"
        )
    expected_blocks = set(
        range(
            plan["confirmatory_block_start"],
            plan["confirmatory_block_start"] + sample_size,
        )
    )
    results = []
    for target, blocks in sorted(grouped.items()):
        selected = [block for block in blocks if block["block"] in expected_blocks]
        if {block["block"] for block in selected} != expected_blocks:
            raise ValueError(f"Missing predeclared blocks for {target}")
        for metric in METRICS:
            margin = (
                plan["build_equivalence_margin"]
                if metric == "build"
                else plan["runtime_equivalence_margin"]
            )
            comparisons = [
                (left, right, False)
                for left, right in itertools.combinations(LEVELS, 2)
            ]
            comparisons.append(("none-a", "none-b", True))
            for left, right, control in comparisons:
                value = log_value if control else treatment_value
                differences = [
                    value(block, right, metric) - value(block, left, metric)
                    for block in selected
                ]
                lower, upper = median_interval(differences, interval_alpha)
                results.append(
                    {
                        "target": target,
                        "metric": metric,
                        "reference": left,
                        "treatment": right,
                        "control": control,
                        "independent_blocks": sample_size,
                        "median_ratio": math.exp(statistics.median(differences)),
                        "confidence_interval_ratio": [
                            math.exp(lower),
                            math.exp(upper) if math.isfinite(upper) else None,
                        ],
                        "margin": margin,
                        "status": classify(lower, upper, margin, control=control),
                        "log_ratios": differences,
                    }
                )
    if len(results) != plan["simultaneous_contrasts"]:
        raise ValueError("Comparison family differs from registration")
    return {
        "study": plan["study_id"],
        "independent_blocks_per_platform": sample_size,
        "look_index": look_index,
        "look_alpha": look_alpha,
        "per_interval_alpha": interval_alpha,
        "resolved": all(result["status"] != "unresolved" for result in results),
        "results": results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", type=Path, nargs="+")
    parser.add_argument("--sample-size", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    plan = json.loads(Path(__file__).with_name("study-plan.json").read_text())
    reports = [json.loads(path.read_text()) for path in args.reports]
    result = analyze(reports, plan, args.sample_size)
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(
        json.dumps(
            {
                key: result[key]
                for key in ("study", "independent_blocks_per_platform", "resolved")
            }
        )
    )


if __name__ == "__main__":
    main()
