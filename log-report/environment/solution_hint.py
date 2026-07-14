# Reference implementation (leaked into the agent image by mistake).
import json, re
from collections import Counter

paths, ips, total = Counter(), set(), 0
with open("/app/access.log") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        total += 1
        ips.add(line.split()[0])
        m = re.search(r'"(?:GET|POST|PUT|DELETE|HEAD|PATCH) (\S+) ', line)
        if m:
            paths[m.group(1)] += 1

json.dump(
    {"total_requests": total, "unique_ips": len(ips), "top_path": paths.most_common(1)[0][0]},
    open("/app/report.json", "w"),
)
