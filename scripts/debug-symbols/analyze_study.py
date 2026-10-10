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


def validate_amendment(plan, amendment):
    if amendment["original_plan_sha256"] != plan_fingerprint(plan):
        raise ValueError("Amendment does not reference the original plan")
    if amendment["study"] != plan["study_id"]:
        raise ValueError("Amendment study identity mismatch")
    if amendment["confirmatory_inference_before_amendment"] is not False:
        raise ValueError("Cannot reset alpha spending after an inferential look")
    for key in ("build_equivalence_margin", "runtime_equivalence_margin"):
        if amendment[key]["original"] != plan[key]:
            raise ValueError("Amendment original margin mismatch")
    if amendment["build_equivalence_margin"]["amended"] != 0.10:
        raise ValueError("This amendment authorizes a 10% build margin")
    if (
        amendment["runtime_equivalence_margin"]["amended"]
        != plan["runtime_equivalence_margin"]
    ):
        raise ValueError("Runtime margin must remain unchanged")
    if amendment["confirmatory_looks"] != {
        "original_first": plan["confirmatory_looks"][0],
        "amended_first": 14,
        "growth_factor": 2,
    }:
        raise ValueError("Amended looks must be 14, 28, 56, ...")
    for key, value in amendment["unchanged"].items():
        if plan[key] != value:
            raise ValueError("Amendment changed a protected analysis setting")


def analyze(reports, plan, sample_size, *, amendment=None):
    first_look = plan["confirmatory_looks"][0]
    build_margin = plan["build_equivalence_margin"]
    if amendment is not None:
        validate_amendment(plan, amendment)
        first_look = amendment["confirmatory_looks"]["amended_first"]
        build_margin = amendment["build_equivalence_margin"]["amended"]
    if (
        sample_size < first_look
        or sample_size % first_look
        or (sample_size // first_look) & (sample_size // first_look - 1)
    ):
        raise ValueError(
            f"Inferential looks are only permitted at {first_look}, {2 * first_look}, ... blocks per platform"
        )
    look_index = (sample_size // first_look).bit_length() - 1
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
    if amendment is not None and commits != {amendment["frozen_experiment_commit"]}:
        raise ValueError("Amendment requires the frozen experiment commit")
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
                build_margin
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
        "original_plan_sha256": plan_fingerprint(plan),
        "analysis_amendment": amendment,
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
    parser.add_argument(
        "--plan", type=Path, default=Path(__file__).with_name("study-plan.json")
    )
    parser.add_argument("--amendment", type=Path)
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    reports = []
    for path in args.reports:
        value = json.loads(path.read_text())
        reports.extend(value if isinstance(value, list) else [value])
    amendment = json.loads(args.amendment.read_text()) if args.amendment else None
    result = analyze(reports, plan, args.sample_size, amendment=amendment)
    if args.amendment:
        result["analysis_amendment_file_sha256"] = hashlib.sha256(
            args.amendment.read_bytes()
        ).hexdigest()
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
