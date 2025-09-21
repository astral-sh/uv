#!/usr/bin/env python3
# NB: LLM code ahead
# /// script
# requires-python = ">=3.13"
# dependencies = ["httpx", "orjson", "tqdm"]
# ///

import asyncio
import csv
from pathlib import Path

import httpx
import orjson
from tqdm.asyncio import tqdm


async def get_latest_version(
    client: httpx.AsyncClient, package_name: str
) -> tuple[str, str | None]:
    try:
        response = await client.get(f"https://pypi.org/pypi/{package_name}/json")
        if response.status_code == 200:
            data = orjson.loads(response.content)
            return package_name, data["info"]["version"]
        else:
            return package_name, None
    except Exception:
        return package_name, None


async def main() -> None:
    input_file = Path("scripts/ecosystem-testing/top-pypi-packages.csv")

    # Read package names
    with open(input_file) as f:
        package_names: list[str] = [row["project"] for row in csv.DictReader(f)]

    print(f"Processing {len(package_names)} packages...")

    # Fetch versions concurrently
    results: dict[str, str | None] = {}
    async with httpx.AsyncClient() as client:
        semaphore = asyncio.Semaphore(50)

        async def fetch(pkg: str) -> tuple[str, str | None]:
            async with semaphore:
                return await get_latest_version(client, pkg)

        tasks = [fetch(pkg) for pkg in package_names]

        for future in tqdm(asyncio.as_completed(tasks), total=len(package_names)):
            name, version = await future
            results[name] = version

    # Write results
    with open("package_versions.csv", "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["package_name", "latest_version"])
        for name in package_names:
            writer.writerow([name, results.get(name, "")])

    success_count = sum(1 for v in results.values() if v)
    print(f"Completed: {success_count}/{len(package_names)} successful")


asyncio.run(main())
