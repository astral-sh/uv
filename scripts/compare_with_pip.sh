#!/usr/bin/env bash

TEMPD=$(mktemp -d)

# `| grep -v " *#"` to ignore the comment when diffing
time RUST_LOG=puffin=debug cargo run --bin puffin -- pip-compile ${1} | grep -v " *#" > $TEMPD/puffin.txt
# > WARNING: --strip-extras is becoming the default in version 8.0.0. To silence this warning, either use --strip-extras
# > to opt into the new default or use --no-strip-extras to retain the existing behavior.
time pip-compile --strip-extras -o - -q ${1} | grep -v " *#" > $TEMPD/pip-compile.txt
diff -u $TEMPD/pip-compile.txt $TEMPD/puffin.txt
