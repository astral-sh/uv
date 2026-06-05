# /// script
# requires-python = ">=3.11"
# dependencies = ["coverage"]
# ///

# Adapted from pyca/cryptography's merge_rust_coverage.py
# See: <https://github.com/pyca/cryptography/blob/0bc628d1e/.github/bin/merge_rust_coverage.py>
# License: <https://github.com/pyca/cryptography/blob/0bc628d1e/LICENSE>

from __future__ import annotations

import argparse
import collections
import collections.abc
import pathlib
import webbrowser

import coverage

CoverageData = collections.abc.Mapping[str, collections.abc.Mapping[int, int]]
ID_ALPHABET = frozenset(
    "_-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
)
# This report uses only source and line records; ignore function, branch, and summary metadata.
IGNORED_RECORDS = {
    "BRDA",
    "BRF",
    "BRH",
    "FN",
    "FNDA",
    "FNF",
    "FNH",
    "LF",
    "LH",
    "TN",
}
REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent


class RustCoveragePlugin(coverage.CoveragePlugin):
    def __init__(self, coverage_data: CoverageData) -> None:
        super().__init__()
        self._data = coverage_data

    def file_reporter(self, filename: str) -> coverage.FileReporter:
        return RustCoverageFileReporter(filename, self._data[filename])


class RustCoverageFileReporter(coverage.FileReporter):
    def __init__(
        self, filename: str, coverage_data: collections.abc.Mapping[int, int]
    ) -> None:
        super().__init__(filename)
        self._data = coverage_data

    def lines(self) -> set[int]:
        return set(self._data)


def tracking_id(value: str) -> str:
    if not value or not set(value) <= ID_ALPHABET:
        raise argparse.ArgumentTypeError(
            "coverage ID must be non-empty and use only valid characters"
        )
    return value


def parse_lcov(path: pathlib.Path) -> CoverageData:
    raw_data: collections.defaultdict[str, collections.defaultdict[int, int]] = (
        collections.defaultdict(lambda: collections.defaultdict(int))
    )
    current_file: str | None = None

    with path.open(encoding="utf-8") as file:
        for line_number, line in enumerate(file, start=1):
            line = line.strip()
            if not line:
                continue
            if line == "end_of_record":
                if current_file is None:
                    raise ValueError(f"{path}:{line_number}: unexpected end_of_record")
                current_file = None
                continue

            prefix, separator, suffix = line.partition(":")
            if not separator:
                raise ValueError(f"{path}:{line_number}: malformed LCOV record")

            if prefix == "SF":
                if current_file is not None:
                    raise ValueError(
                        f"{path}:{line_number}: source record was not terminated"
                    )
                source = pathlib.Path(suffix)
                if not source.is_absolute():
                    source = REPO_ROOT / source
                source = source.resolve()
                if not source.is_relative_to(REPO_ROOT):
                    raise ValueError(
                        f"{path}:{line_number}: source is outside the repository: {source}"
                    )
                if not source.is_file():
                    raise ValueError(
                        f"{path}:{line_number}: source file does not exist: {source}"
                    )
                current_file = str(source)
            elif prefix == "DA":
                if current_file is None:
                    raise ValueError(
                        f"{path}:{line_number}: DA record has no source file"
                    )
                fields = suffix.split(",")
                if len(fields) < 2:
                    raise ValueError(f"{path}:{line_number}: malformed DA record")
                try:
                    source_line = int(fields[0])
                    count = int(fields[1])
                except ValueError as error:
                    raise ValueError(
                        f"{path}:{line_number}: malformed DA record"
                    ) from error
                raw_data[current_file][source_line] += count
            elif prefix not in IGNORED_RECORDS:
                raise ValueError(
                    f"{path}:{line_number}: unsupported LCOV record: {prefix}"
                )

    if current_file is not None:
        raise ValueError(f"{path}: unterminated source record")
    if not raw_data:
        raise ValueError(f"{path}: no line coverage records found")

    return raw_data


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate terminal and HTML reports from a Rust coverage run."
    )
    parser.add_argument("id", type=tracking_id, help="Coverage tracking ID")
    parser.add_argument(
        "--open",
        action="store_true",
        help="Open the generated HTML report in the default browser",
    )
    args = parser.parse_args()

    lcov_path = REPO_ROOT / "target" / "coverage" / "lcov" / f"{args.id}.lcov"
    if not lcov_path.is_file():
        parser.error(f"LCOV file does not exist: {lcov_path}")

    raw_data = parse_lcov(lcov_path)
    covered_lines = {
        filename: {line for line, count in lines.items() if count > 0}
        for filename, lines in raw_data.items()
    }

    plugin_name = "None.RustCoveragePlugin"
    cov = coverage.Coverage(
        data_file=None,
        config_file=False,
        plugins=[
            lambda registry: registry.add_file_tracer(RustCoveragePlugin(raw_data))
        ],
    )
    data = cov.get_data()
    data.add_lines(covered_lines)
    data.add_file_tracers(dict.fromkeys(raw_data, plugin_name))

    cov.report(show_missing=True)
    html_directory = REPO_ROOT / "target" / "coverage" / "html" / args.id
    cov.html_report(directory=str(html_directory))
    html_report = html_directory / "index.html"
    print(f"HTML report: {html_report}")
    if args.open:
        webbrowser.open(html_report.as_uri())


if __name__ == "__main__":
    main()
