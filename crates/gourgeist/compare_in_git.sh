#!/usr/bin/env bash

set -e

virtualenv_command() {
  virtualenv -p 3.11 compare_venv # --no-pip --no-setuptools --no-wheel
}
rust_command() {
  cargo run -- -p 3.11 compare_venv # --bare
}

rm -rf compare_venv
virtualenv_command
rm compare_venv/.gitignore
git -C compare_venv init
git -C compare_venv add -A
git -C compare_venv commit -q -m "Initial commit"
rm -r compare_venv/* # This skips the hidden .git
mkdir -p target
mv compare_venv target/compare_venv2
rust_command
rm compare_venv/.gitignore
cp -r compare_venv/* target/compare_venv2
rm -r compare_venv
mv target/compare_venv2 compare_venv
git -C compare_venv/ status

