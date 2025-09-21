#!/usr/bin/env -S uv run --script
# NB: LLM code ahead
# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "httpx>=0.28,<0.29",
#     "orjson>=3,<4",
#     "tqdm>=4,<5"
# ]
# ///

import argparse
import asyncio
import csv
from pathlib import Path

import orjson
from httpx import AsyncClient, HTTPError
from tqdm.asyncio import tqdm


async def get_latest_version(
    client: AsyncClient, package_name: str
) -> tuple[str, str | None]:
    try:
        response = await client.get(f"https://pypi.org/pypi/{package_name}/json")
        response.raise_for_status()
        data = orjson.loads(response.content)
        return package_name, data["info"]["version"]
    except HTTPError as e:
        print(f"Error fetching latest version for {package_name}: {e}")
        return package_name, None


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--input-file",
        type=Path,
        default=Path("scripts/ecosystem-testing/top-pypi-packages.csv"),
    )
    parser.add_argument(
        "--output-file",
        type=Path,
        default=Path("package_versions.csv"),
    )
    args = parser.parse_args()

    with args.input_file.open() as f:
        package_names = [row["project"] for row in csv.DictReader(f)]

    print(f"Fetching latest versions for {len(package_names)} packages")

    versions: dict[str, str | None] = {}
    async with AsyncClient() as client:
        semaphore = asyncio.Semaphore(50)

        async def fetch(pkg: str) -> tuple[str, str | None]:
            async with semaphore:
                return await get_latest_version(client, pkg)

        tasks = [fetch(pkg) for pkg in package_names]

        for future in tqdm(asyncio.as_completed(tasks), total=len(package_names)):
            name, version = await future
            versions[name] = version

    with args.output_file.open("w") as f:
        writer = csv.DictWriter(f, ["package_name", "latest_version"])
        writer.writeheader()
        for name, version in versions.items():
            writer.writerow({"package_name": name, "latest_version": version})

    success_count = sum(v is not None for v in versions.values())
    print(f"Found version for {success_count}/{len(package_names)} packages")


asyncio.run(main())
