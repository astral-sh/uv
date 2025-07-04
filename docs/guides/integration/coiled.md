---
title: Using uv with Coiled
description:
  A complete guide to using uv with Coiled to manage Python dependencies and deploy serverless
  scripts.
---

# Using uv with Coiled

[Coiled](https://coiled.io?utm_source=uv-docs) is a serverless, UX-focused cloud computing platform
that makes it easy to run code on cloud hardware (AWS, GCP, and Azure).

This guide shows how to run Python scripts on the cloud using uv for software dependency management
and Coiled for cloud deployment.

## Managing software with uv

Here's a `process.py` script that uses [`pandas`](https://pandas.pydata.org/docs/) to load a Parquet
data file hosted in a public bucket on S3 and prints the first few rows:

```python title="process.py"

import pandas as pd

df = pd.read_parquet(
    "s3://coiled-data/uber/part.0.parquet",
    storage_options={"anon": True},
)
print(df.head())
```

We'll use this concrete example throughout the rest of the guide, but know that the script contents
could be _any_ Python code.

Running this script requires `pandas`, `pyarrow`, and `s3fs`. uv makes it easy to embed these
dependencies directly in the script using [PEP 273](https://peps.python.org/pep-0723/) inline
metadata with the [`uv add --script` command](../scripts.md#declaring-script-dependencies).

```bash
$ uv add --script process.py pandas pyarrow s3fs
```

which adds these comments with the specified dependencies to the script

```python title="process.py" hl_lines="1-8"

# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "pandas",
#   "pyarrow",
#   "s3fs",
# ]
# ///

import pandas as pd

df = pd.read_parquet(
    "s3://coiled-data/uber/part.0.parquet",
    storage_options={"anon": True},
)
print(df.head())
```

We then use `uv run` to run the script locally

```bash
$ uv run process.py
```

uv automatically creates a virtual environment, installs the dependencies, and then runs
`process.py` in that environment. Here's the output we get:

```
hvfhs_license_num dispatching_base_num originating_base_num    request_datetime   on_scene_datetime  ... shared_request_flag shared_match_flag  access_a_ride_flag  wav_request_flag  wav_match_flag
__null_dask_index__                                                                                                      ...
18979859                       HV0003               B02875               B02875 2019-05-26 23:29:35 2019-05-26 23:30:33  ...                   N                 N                 NaN                 N             NaN
18979860                       HV0003               B02875               B02875 2019-05-26 23:56:48 2019-05-26 23:57:03  ...                   N                 N                 NaN                 N             NaN
18979861                       HV0003               B02765               B02765 2019-05-26 23:56:35 2019-05-26 23:56:41  ...                   N                 N                 NaN                 N             NaN
18979862                       HV0003               B02682               B02682 2019-05-26 22:52:34 2019-05-26 23:10:38  ...                   Y                 N                 NaN                 N             NaN
18979863                       HV0002               B03035               B03035 2019-05-26 23:16:34 1970-01-01 00:00:00  ...                   N                 N                   N                 N             NaN

[5 rows x 24 columns]
```

What's nice about this is we didn't have to think about managing local virtual environments
ourselves and the dependencies needed are included directly in the script which makes things nicely
self-contained.

That's really all we need for running scripts locally. However, there are many common use cases
where resources beyond what's available on our local workstation are needed, like:

- Processing large amounts of cloud-hosted data
- Needing accelerated hardware like GPUs or a big machine with more memory
- Running the same script with hundreds or thousands of different inputs, in parallel

In these situations running scripts directly on cloud hardware is often a good solution.

## Running on the cloud with Coiled

Similar to how uv makes it straightforward to handle Python dependency management, Coiled makes it
straightforward to handle running code on cloud hardware.

Let's start by authenticating whatever machine you're running on with Coiled using the
[`coiled login` CLI](https://docs.coiled.io/user_guide/api.html?utm_source=uv-docs#coiled-login)
(you'll be prompted to create a Coiled account if you don't already have one -- and it's totally
free to start using Coiled):

```bash
$ uvx coiled login
```

!!! tip

    While Coiled supports AWS, GCP, and Azure, this example assumes AWS is being used
    (see the `region` option below). If you're new to Coiled, you'll automatically have
    access to a free account running on AWS. If you're not running on AWS, you can either use
    a valid `region` for your cloud provider or remove the `region` line below.

To have Coiled run our script on a VM on AWS, we'll add these two comments:

```python title="process.py" hl_lines="1-2"

# COILED container ghcr.io/astral-sh/uv:debian-slim
# COILED region us-east-2

# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "pandas",
#   "pyarrow",
#   "s3fs",
# ]
# ///

import pandas as pd

df = pd.read_parquet(
    "s3://coiled-data/uber/part.0.parquet",
    storage_options={"anon": True},
)
print(df.head())
```

They tell Coiled to use the official
[uv Docker image](https://github.com/astral-sh/uv/pkgs/container/uv) when running the script (this
makes sure uv is installed) and to run the script in the `us-east-2` region on AWS (where this data
file happens to live) to avoid any data egress. There are several other options we could have
specified here like VM instance type (the default is a 4-core VM with 16 GiB of memory), whether to
use spot instance, etc. See the
[Coiled Batch docs](https://docs.coiled.io/user_guide/batch.html?utm_source=uv-docs) for more
details.

Finally we use the
[`coiled batch run` CLI](https://docs.coiled.io/user_guide/api.html?utm_source=uv-docs#coiled-batch-run)
to run our existing `uv run` command on a cloud VM.

```bash hl_lines="1"

$ uvx coiled batch run \
    uv run process.py
```

The same exact thing that happened locally before now happens on a cloud VM on AWS, only this time
the script is faster because we didn't have to transfer any data from S3 to a local laptop.

![Coiled UI](https://docs.coiled.io/_images/uv-coiled.png)

For more details on Coiled, and how it can be used in other use cases, see the
[Coiled documentation](https://docs.coiled.io?utm_source=uv-docs).
