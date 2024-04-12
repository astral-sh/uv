#!/bin/bash

set -e

cd "$(git rev-parse --show-toplevel)"

mkdir -p soft-serve/data
openssl req -x509 -newkey rsa:4096 -keyout soft-serve/data/key.pem -out soft-serve/data/cert.pem -sha256 -days 3650 -nodes -subj "/C=XX/ST=StateName/L=CityName/O=CompanyName/OU=CompanySectionName/CN=localhost"

SOFT_SERVE_DATA_PATH=soft-serve/data SOFT_SERVE_INITIAL_ADMIN_KEYS=~/.ssh/id_ed25519.pub soft-serve/soft serve
ssh-keygen -f ~/.ssh/known_hosts -R "[localhost]:23231"
ssh -p 23231 localhost settings anon-access admin-access
# export GIT_SSL_CAINFO=soft-serve/data/cert.pem git

# git\+https://github.com/[^/]+/([a-z0-9\-_]+)
# git+https://localhost:23232/$1

# rg 'git\+https://github.com/[^/]+/([a-z0-9\-_]+)' --files-with-matches | xargs sed -i -E 's|git\+https://github.com/[^/]+/([a-z0-9\-_]+)|git+https://localhost:23232/\1|g'
