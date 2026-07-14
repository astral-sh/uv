#!/bin/bash

pytest /tests/test_outputs.py -rA --ctrf /app/ctrf.json

if [ $? -eq 0 ]; then
  echo 1 > /app/reward.txt
else
  echo 0 > /app/reward.txt
fi