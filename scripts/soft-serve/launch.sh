#!/bin/bash

set -e

cd "$(git rev-parse --show-toplevel)"

cp scripts/soft-serve/config.yaml soft-serve/data/config.yaml
SOFT_SERVE_DATA_PATH=soft-serve/data SOFT_SERVE_INITIAL_ADMIN_KEYS=~/.ssh/id_ed25519.pub soft-serve/soft serve
