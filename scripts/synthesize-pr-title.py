# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "openai>=1.51.0",
# ]
# ///
"""
This script is used to generate a PR title for the `sync-python-releases.yml` action.
It sends the generated diff to the OpenAI API and retrieves the generated title.

This script requires an API key from OpenAI. The key should be stored in the `OPENAI_API_KEY` environment variable.
"""

import os
import subprocess
import sys

import openai

PROMPT = """
Generate a concise and descriptive commit message title based on the given git diff.

Reminders about the git diff format:
For every file, there are a few metadata lines, like (for example):
```
diff --git a/crates/uv-python/download-metadata.json b/crates/uv-python/download-metadata.json
index f3958158..30723397 100644
--- a/crates/uv-python/download-metadata.json
+++ b/crates/uv-python/download-metadata.json
```
This means that `crates/uv-python/download-metadata.json` was modified in this commit.
Note that this is only an example.
Then there is a specifier of the lines that were modified.
A line starting with `+` means it was added.
A line that starting with `-` means that line was deleted.
A line that starts with neither `+` nor `-` is code given for context and better understanding.
It is not part of the diff.

The diff contains the changes made to `download-metadata.json` file, which lists CPython and PyPy releases.
The title should be in the format: [Add/Update/Remove] [CPython/PyPy] [version number] "downloads".,

EXAMPLE TITLES:
```
Add CPython 3.13.0 downloads
Update PyPy v7.3.16 to v7.3.17
Add CPython 3.13.1 and 3.12.8 downloads
Add CPython 3.13.1, update 3.12.8, 3.11.12 and 3.10.20 downloads
```

THE GIT DIFF:
```
{diff}
```

Remember to write only one line, no more than 60 characters.
THE COMMIT MESSAGE TITLE:
"""
DEFAULT_TITLE = "Sync latest Python releases"
DOWNLOADS_PATH = "crates/uv-python/download-metadata.json"


def ask() -> str:
    diff = subprocess.check_output(
        ["git", "diff", "HEAD~1", "HEAD", "--", DOWNLOADS_PATH], text=True
    )
    if diff.count("\n") > 2000:
        raise ValueError("The diff is too large to process")

    client = openai.Client(api_key=os.environ["OPENAI_API_KEY"])
    response = client.chat.completions.create(
        model="gpt-4o-mini",
        max_completion_tokens=60,
        temperature=0.5,
        messages=[
            {"role": "user", "content": PROMPT.format(diff=diff)},
        ],
    )

    return response.choices[0].text.strip()


def main() -> None:
    try:
        response = ask()
    except Exception as e:
        print(f"An error occurred: {e}", file=sys.stderr)
        response = DEFAULT_TITLE
    print(response)


if __name__ == "__main__":
    main()
