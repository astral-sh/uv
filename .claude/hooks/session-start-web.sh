#!/bin/bash
set -euo pipefail

# Install `gh`
if ! command -v gh &> /dev/null; then
    GH_VERSION="2.83.2"
    mkdir -p ~/.local/bin
    curl -sL "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_amd64.tar.gz" -o /tmp/gh.tar.gz
    tar -xzf /tmp/gh.tar.gz -C /tmp
    mv /tmp/gh_${GH_VERSION}_linux_amd64/bin/gh ~/.local/bin/
    rm -rf /tmp/gh.tar.gz /tmp/gh_${GH_VERSION}_linux_amd64
fi
