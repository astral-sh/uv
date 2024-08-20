#!/bin/bash

for i in {1..200}; do
    ./target/debug/uv pip compile -q requirements.in -o requirements.txt &
done

wait
