#!/usr/bin/env bash

curl https://hugovk.github.io/top-pypi-packages/top-pypi-packages.min.json | jq -r ".rows | .[].project" > pypi_8k_downloads.txt
curl https://gist.githubusercontent.com/charliermarsh/07afd9f543dfea68408a4a42cede4be4/raw/6639cd58a2e10d6bb7821f891f00322c8630b60a/pypi_10k_most_dependents.txt > pypi_10k_most_dependents.txt
