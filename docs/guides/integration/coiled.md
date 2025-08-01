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

## Scripts with inline metadata

!!! note

    We'll use this concrete example throughout this guide, but any Python script can be used with
    uv and Coiled.

We'll use the following script as an example:

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

The script uses [`pandas`](https://pandas.pydata.org/docs/) to load a Parquet file hosted in a
public bucket on S3, then prints the first few rows. It uses
[inline script metadata](https://peps.python.org/pep-0723/) to enumerate its dependencies.

When running this script locally, e.g., with:

```bash
$ uv run process.py
```

uv will automatically create a virtual environment and installs its dependencies.

To learn more about using inline script metadata with uv, see the
[script guide](../scripts.md#declaring-script-dependencies).

## Running scripts on the cloud with Coiled

Using inline script metadata makes the script fully self-contained: it includes the information that
is needed to run it. This makes it easier to run on other machines, like a machine in the cloud.

There are many use cases where resources beyond what's available on a local workstation are needed,
e.g.:

- Processing large amounts of cloud-hosted data
- Needing accelerated hardware like GPUs or a big machine with more memory
- Running the same script with hundreds or thousands of different inputs, in parallel

Coiled makes it simple to run code on cloud hardware.

First, authenticate with Coiled using
[`coiled login`](https://docs.coiled.io/user_guide/api.html?utm_source=uv-docs#coiled-login) :

```bash
$ uvx coiled login
```

You'll be prompted to create a Coiled account if you don't already have one — it's free to start
using Coiled.

To instruct Coiled to run the script on a virtual machine on AWS, add two comments to the top:

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

!!! tip

    While Coiled supports AWS, GCP, and Azure, this example assumes AWS is being used
    (see the `region` option below). If you're new to Coiled, you'll automatically have
    access to a free account running on AWS. If you're not running on AWS, you can either use
    a valid `region` for your cloud provider or remove the `region` line below.

The comments tell Coiled to use the official [uv Docker image](../integration/docker.md) when
running the script (ensuring uv is available) and to run in the `us-east-2` region on AWS (where
this example data file happens to live) to avoid any data egress.

To run the script, use
[`coiled batch run`](https://docs.coiled.io/user_guide/api.html?utm_source=uv-docs#coiled-batch-run)
to execute the `uv run` command in the cloud:

```bash hl-lines="1"
$ uvx coiled batch run \
    uv run process.py
```

<!-- TODO
This command returns immediately, it doesn't wait for the job to finish and it doesn't seem
like there's a flag for that. It also doesn't happen much faster, because a remote job needs to
spawn. I also wasn't sure how to get the logs for the job. I eventually found it with
`uvx coiled batch logs 1067394`. I also tried `uvx coiled batch wait 1067394`, but it didn't have
an option to show logs and that retrieving the id in the first place was a bit challenging, e.g.,
`uvx coiled batch status` shows it as a cluster ID at the top but that's not obvious.

I presume some of these problems are because this API is designed around running multiple batch
jobs, however, if the user experience for running the script locally is that it waits for execution
and shows the output, then we need to address that difference in user experience here.

It looks like `uvx coiled batch run -- uv run process.py` isn't supported (using the `--` as a
separator), I wanted to use that for a single-line commmand that still separated the `uv run`
command.
-->

The same exact thing that happened locally before now happens on a cloud VM on AWS, only this time
the script is faster because we didn't have to transfer any data from S3 to a local laptop.

There are other options we could have specified, like, the instance type (the default is a 4-core
virtual machine with 16 GiB of memory), whether to use spot instance, etc. See the
[Coiled Batch documentation](https://docs.coiled.io/user_guide/batch.html?utm_source=uv-docs) for
more details.

![Coiled UI](https://docs.coiled.io/_images/uv-coiled.png)

For more details on Coiled, and how it can be used in other use cases, see the
[Coiled documentation](https://docs.coiled.io?utm_source=uv-docs).
