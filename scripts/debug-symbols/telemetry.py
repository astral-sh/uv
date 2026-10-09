"""Sample build resource use without changing compiler arguments.

Tree CPU and I/O totals are lower bounds: processes that exit between samples can
be missed. RSS is a sampled sum and may count shared pages more than once. Host
metrics include unrelated processes. None of these fields is a CPU-throttling or
memory-pressure diagnosis by itself.
Page-fault counters may include soft faults, and some platform swap counters are
unsupported zeros; they cannot establish absence of paging.
"""

import json
import os
import platform
import subprocess
import threading
import time
from collections import Counter
from pathlib import Path

try:
    import psutil
except ImportError:
    psutil = None

try:
    import resource
except ImportError:
    resource = None


def machine_info():
    if psutil is None:
        raise RuntimeError("Resource sampling requires psutil==7.0.0")
    if platform.system() == "Windows":
        command = [
            "powershell",
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Processor | Select-Object Name,Manufacturer,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed | ConvertTo-Json -Compress",
        ]
    elif platform.system() == "Darwin":
        command = ["sysctl", "-n", "machdep.cpu.brand_string"]
    else:
        command = ["lscpu", "--json"]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    return {
        "platform": platform.platform(),
        "logical_cpus": psutil.cpu_count(),
        "physical_cpus": psutil.cpu_count(logical=False),
        "physical_memory_bytes": psutil.virtual_memory().total,
        "cpu_description": result.stdout.strip(),
        "cpu_description_exit_code": result.returncode,
        "psutil_version": psutil.__version__,
        "runner_name": os.environ.get("RUNNER_NAME"),
    }


class PhaseSampler:
    def __init__(self, path, *, enabled=True, interval=1.0):
        self.path = Path(path)
        self.enabled = enabled
        self.interval = interval
        self.stop = threading.Event()
        self.processes = {}
        self.errors = Counter()
        self.samples = 0
        self.peak_rss = 0
        self.summary = None

    def sample(self):
        rss = 0
        alive = 0
        try:
            descendants = self.root.children(recursive=True)
        except (psutil.Error, OSError) as error:
            self.errors[f"children:{type(error).__name__}"] += 1
            descendants = []
        for process in descendants:
            try:
                with process.oneshot():
                    key = (process.pid, process.create_time())
                    cpu = process.cpu_times()
                    memory = process.memory_info()
                    values = {
                        "cpu_user_seconds": cpu.user,
                        "cpu_system_seconds": cpu.system,
                    }
                    for field in ("num_page_faults", "pfaults", "pageins"):
                        if hasattr(memory, field):
                            values[field] = getattr(memory, field)
                    try:
                        io = process.io_counters()
                        values.update(
                            {"read_bytes": io.read_bytes, "write_bytes": io.write_bytes}
                        )
                    except (AttributeError, NotImplementedError, psutil.Error, OSError):
                        self.errors["process_io_unavailable"] += 1
                    previous = self.processes.setdefault(key, {})
                    for field, value in values.items():
                        previous[field] = max(previous.get(field, 0), value)
                    rss += memory.rss
                    alive += 1
            except (psutil.Error, OSError) as error:
                self.errors[type(error).__name__] += 1
        self.peak_rss = max(self.peak_rss, rss)
        totals = Counter()
        for values in self.processes.values():
            totals.update(values)
        memory = psutil.virtual_memory()
        counters = {}
        for name, function in (
            ("host_swap", psutil.swap_memory),
            ("host_disk_io", psutil.disk_io_counters),
        ):
            try:
                result = function()
                counters[name] = result._asdict() if result else None
            except (psutil.Error, OSError, NotImplementedError) as error:
                self.errors[f"{name}:{type(error).__name__}"] += 1
                counters[name] = None
        sample = {
            "unix_time_seconds": time.time(),
            "elapsed_seconds": time.monotonic() - self.started,
            "host_cpu_percent": psutil.cpu_percent(),
            "host_cpu_times": psutil.cpu_times()._asdict(),
            "host_available_memory_bytes": memory.available,
            "host_used_memory_bytes": memory.used,
            **counters,
            "tree_rss_bytes": rss,
            "live_descendants": alive,
            "sampled_tree_counters_lower_bound": dict(totals),
        }
        self.stream.write(json.dumps(sample) + "\n")
        self.stream.flush()
        self.samples += 1

    def collect(self):
        psutil.cpu_percent()
        while not self.stop.is_set():
            try:
                self.sample()
            except (psutil.Error, OSError, ValueError, RuntimeError) as error:
                # A telemetry failure must be visible without terminating a build.
                self.errors[type(error).__name__] += 1
            self.stop.wait(self.interval)

    def __enter__(self):
        if not self.enabled:
            return self
        if psutil is None:
            raise RuntimeError("Resource sampling requires psutil==7.0.0")
        self.root = psutil.Process()
        self.started = time.monotonic()
        self.before = resource.getrusage(resource.RUSAGE_CHILDREN) if resource else None
        self.stream = self.path.open("w", encoding="utf-8")
        self.thread = threading.Thread(target=self.collect, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *exception):
        if not self.enabled:
            return
        self.stop.set()
        self.thread.join()
        self.stream.close()
        totals = Counter()
        for values in self.processes.values():
            totals.update(values)
        self.summary = {
            "samples": self.samples,
            "interval_seconds": self.interval,
            "wall_seconds": time.monotonic() - self.started,
            "sampled_peak_tree_rss_bytes": self.peak_rss,
            "observed_processes": len(self.processes),
            "sampled_tree_counters_lower_bound": dict(totals),
            "errors": dict(self.errors),
        }
        if resource:
            after = resource.getrusage(resource.RUSAGE_CHILDREN)
            self.summary["reaped_children_rusage_delta"] = {
                field: getattr(after, field) - getattr(self.before, field)
                for field in (
                    "ru_utime",
                    "ru_stime",
                    "ru_minflt",
                    "ru_majflt",
                    "ru_inblock",
                    "ru_oublock",
                    "ru_nvcsw",
                    "ru_nivcsw",
                )
            }
        self.path.with_suffix(".json").write_text(
            json.dumps(self.summary, indent=2) + "\n"
        )
