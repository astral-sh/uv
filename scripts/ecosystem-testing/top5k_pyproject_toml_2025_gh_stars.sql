# BigQuery SQL for top5k_pyproject_toml_2025_gh_stars.csv
# Run in https://console.cloud.google.com/bigquery
SELECT
  f.repo_name,
  f.ref,
  COUNT(e.id) AS stars
FROM
  `bigquery-public-data.github_repos.files` f
    JOIN
  `githubarchive.month.2025*` e
  ON
    f.repo_name = e.repo.name
WHERE
  f.path = 'pyproject.toml'
  AND e.type = 'WatchEvent'
GROUP BY
  f.repo_name, f.ref
ORDER BY
  stars DESC
  LIMIT 5000;
