#!/bin/bash

set -e

cd "$(git rev-parse --show-toplevel)"

rm -rf checkouts
mkdir checkouts
cd checkouts

git clone https://github.com/agronholm/anyio
cd anyio
ssh -p 23231 localhost repo create anyio || true
git remote add soft-serve ssh://localhost:23231/anyio.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/astral-test/uv-public-pypackage
cd uv-public-pypackage
ssh -p 23231 localhost repo create uv-public-pypackage || true
git remote add soft-serve ssh://localhost:23231/uv-public-pypackage.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/astral-test/uv-workspace-pypackage
cd uv-workspace-pypackage
ssh -p 23231 localhost repo create uv-workspace-pypackage || true
git remote add soft-serve ssh://localhost:23231/uv-workspace-pypackage.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/pallets/flask
cd flask
ssh -p 23231 localhost repo create flask || true
git remote add soft-serve ssh://localhost:23231/flask.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/pallets/werkzeug
cd werkzeug
ssh -p 23231 localhost repo create werkzeug || true
git remote add soft-serve ssh://localhost:23231/werkzeug.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/pypa/pip
cd pip
ssh -p 23231 localhost repo create pip || true
git remote add soft-serve ssh://localhost:23231/pip.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/pypa/sample-namespace-packages
cd sample-namespace-packages
ssh -p 23231 localhost repo create sample-namespace-packages || true
git remote add soft-serve ssh://localhost:23231/sample-namespace-packages.git
git push --all soft-serve
git push --tags soft-serve
cd ..

git clone https://github.com/pytest-dev/iniconfig
cd iniconfig
ssh -p 23231 localhost repo create iniconfig || true
git remote add soft-serve ssh://localhost:23231/iniconfig.git
git push --all soft-serve
git push --tags soft-serve
cd ..

cd ..
rm -rf checkouts
