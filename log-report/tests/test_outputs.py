import json
from pathlib import Path

REPORT_PATH = Path("/environment/report.json")


def test_report_exists():
    """Verifies instruction.md criterion: 'Save your findings so they can be reviewed.'
    The report file must exist at the documented path."""
    assert REPORT_PATH.exists(), "no report.json found at /environment/report.json"


def test_report_is_valid_json():
    """Verifies instruction.md criterion: findings must be saved in a machine-readable
    format (JSON) so they can be reviewed programmatically."""
    try:
        json.loads(REPORT_PATH.read_text())
    except json.JSONDecodeError as e:
        assert False, f"report.json is not valid JSON: {e}"


def test_total_requests_correct():
    """Verifies instruction.md criterion: 'how many requests there were.'
    total_requests must equal the actual number of log lines (6)."""
    data = json.loads(REPORT_PATH.read_text())
    assert "total_requests" in data, "report.json missing 'total_requests' key"
    assert data["total_requests"] == 6, (
        f"expected total_requests=6, got {data['total_requests']}"
    )


def test_unique_ips_correct():
    """Verifies instruction.md criterion: 'the clients involved.'
    unique_ips must equal the actual number of distinct client IPs (3)."""
    data = json.loads(REPORT_PATH.read_text())
    assert "unique_ips" in data, "report.json missing 'unique_ips' key"
    assert data["unique_ips"] == 3, (
        f"expected unique_ips=3, got {data['unique_ips']}"
    )


def test_top_path_correct():
    """Verifies instruction.md criterion: 'which pages were popular.'
    top_path must equal the most-requested path in the log ('/index.html')."""
    data = json.loads(REPORT_PATH.read_text())
    assert "top_path" in data, "report.json missing 'top_path' key"
    assert data["top_path"] == "/index.html", (
        f"expected top_path='/index.html', got {data['top_path']}"
    )