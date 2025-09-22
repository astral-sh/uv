# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "httpx>=0.28.1,<0.29.0",
#     "tqdm>=4.67.1,<5.0.0",
# ]
# ///

import argparse
import asyncio
import csv
import shutil
from dataclasses import dataclass
from pathlib import Path

import httpx
from httpx import AsyncClient
from tqdm.auto import tqdm


@dataclass
class Repository:
    org: str
    repo: str
    ref: str


async def fetch_pyproject(
    client: AsyncClient, repository: Repository, output_dir: Path
):
    url = f"https://raw.githubusercontent.com/{repository.org}/{repository.repo}/{repository.ref}/pyproject.toml"
    try:
        response = await client.get(url)
        response.raise_for_status()
    except httpx.HTTPError as e:
        # The bigquery data is sometimes missing the master -> main transition
        url = f"https://raw.githubusercontent.com/{repository.org}/{repository.repo}/refs/heads/main/pyproject.toml"
        try:
            response = await client.get(url)
            response.raise_for_status()
        except httpx.HTTPError:
            # Ignore the error from the main fallback if it didn't work
            if hasattr(e, "response") and e.response.status_code == 404:
                tqdm.write(
                    f"Not found: https://github.com/{repository.org}/{repository.repo}"
                )
            else:
                tqdm.write(
                    f"Error for https://github.com/{repository.org}/{repository.repo}: {e}"
                )
            return None

    output_dir.joinpath(f"{repository.repo}.toml").write_text(response.text)
    return True


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, default=Path("top500_2025_gh_stars.csv"))
    parser.add_argument("--output", type=Path, default=Path("pyproject_toml"))
    args = parser.parse_args()

    with args.input.open() as f:
        repositories = []
        seen = set()
        for row in csv.DictReader(f):
            if row["repo_name"] in seen:
                continue
            seen.add(row["repo_name"])
            repositories.append(
                Repository(
                    org=row["repo_name"].split("/")[0],
                    repo=row["repo_name"].split("/")[1],
                    ref=row["ref"],
                )
            )

    if args.output.exists():
        shutil.rmtree(args.output)
    args.output.mkdir(parents=True)
    args.output.joinpath(".gitignore").write_text("*")

    semaphore = asyncio.Semaphore(50)

    async def fetch_with_semaphore(
        client: AsyncClient, repository: Repository, output_dir: Path
    ):
        async with semaphore:
            return await fetch_pyproject(client, repository, output_dir)

    async with httpx.AsyncClient() as client:
        with tqdm(total=len(repositories)) as pbar:
            tasks = [
                fetch_with_semaphore(client, repository, args.output)
                for repository in repositories
            ]
            results = []
            for future in asyncio.as_completed(tasks):
                results.append(await future)
                pbar.update(1)

    success = sum(1 for result in results if result is True)
    print(f"Successes: {success}/{len(repositories)}")


if __name__ == "__main__":
    asyncio.run(main())
