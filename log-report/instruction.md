There is an access log at `access.log` in the working directory. Analyze the traffic and produce a JSON report with exactly these three keys:

`total_requests`: the total number of log lines (requests) in the file.
`unique_ips`: the number of distinct client IP addresses that made requests.
`top_path`: the single most frequently requested path (e.g. "/index.html").

Save this JSON object to `/app/report.json`.

You have 120 seconds to complete this task. Do not cheat by using online solutions or hints specific to this task.