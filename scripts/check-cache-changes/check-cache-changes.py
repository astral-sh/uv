"""
Check that the uv cache keys and layout didn't change accidentally.

This check may fail when intentionally changing the cache layout, it is intended to
catch accidental modification of cache keys.
"""

import os
import shutil
import sys
from argparse import ArgumentParser
from pathlib import Path
from subprocess import CalledProcessError, check_call


def main():
    parser = ArgumentParser()
    parser.add_argument("new_uv", help="The new uv binary")
    parser.add_argument(
        "--old-version",
        help="Optionally, compare to a specific uv version instead of the latest version",
    )

    args = parser.parse_args()
    # Ensure the uv path, if relative, has the right root
    new_uv = Path(os.getcwd()).joinpath(args.new_uv)
    work_dir = Path(__file__).parent
    cache_dir = work_dir.joinpath(".cache")
    if cache_dir.is_dir():
        shutil.rmtree(cache_dir)
    env = {"UV_CACHE_DIR": str(cache_dir), **os.environ}

    if args.old_version:
        old_uv = f"uv@{args.old_version}"
    else:
        old_uv = "uv@latest"

    # Prime the cache
    print(f"\nPriming the cache with {old_uv}\n")
    check_call([new_uv, "tool", "run", old_uv, "sync"], cwd=work_dir, env=env)
    # This should always pass, even if the cache changed
    print(f"\nUsing the cache offline with {old_uv}\n")
    shutil.rmtree(work_dir.joinpath(".venv"))
    check_call(
        [new_uv, "tool", "run", old_uv, "sync", "--offline"], cwd=work_dir, env=env
    )
    # Check that the new uv version can use the old cache
    print(f"\nUsing the cache offline with {new_uv}\n")
    shutil.rmtree(work_dir.joinpath(".venv"))
    try:
        check_call([new_uv, "sync", "--offline"], cwd=work_dir, env=env)
    except CalledProcessError:
        print(
            'Cache layout changed. If this is intentional, add the "cache-change" label to your PR'
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
