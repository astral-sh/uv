#!/usr/bin/env bash

curl https://hugovk.github.io/top-pypi-packages/top-pypi-packages-30-days.min.json | jq -r ".rows | .[].project" > pypi_top_8k_flat.txt

