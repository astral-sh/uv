#!/bin/bash
set -x

############################################
# DSI CONSULTING INC. Project setup script #
############################################
# This script creates standard analysis and output directories
# for a new project. It also creates a README file with the
# project name and a brief description of the project.
# Then it unzips the raw data provided by the client.

# 1. Remove existing newproject folder if exists
if [ -d newproject ]; then
  echo "Recreating the newproject directory"
  rm -rf newproject
fi
mkdir newproject
cd newproject

# 2. Create analysis and output folders
mkdir -p analysis output
touch README.md
touch analysis/main.py

# 3. Download client data and unzip
curl -Lo rawdata.zip https://github.com/UofT-DSI/shell/raw/refs/heads/main/02_activities/assignments/rawdata.zip
unzip -q rawdata.zip

###########################################
# 4. Create data/raw and move rawdata there
mkdir -p data/raw
mv rawdata data/raw/

###########################################
# 5. Copy server logs to processed folder
mkdir -p data/processed/server_logs
cp data/raw/rawdata/*server*.log data/processed/server_logs/

# 6. Copy user and event logs to processed folder
mkdir -p data/processed/user_logs
cp data/raw/rawdata/*user*.log data/processed/user_logs/

mkdir -p data/processed/event_logs
cp data/raw/rawdata/*event*.log data/processed/event_logs/

# 7. Remove files containing "ipaddr"
rm -f data/raw/rawdata/*ipaddr*
rm -f data/processed/user_logs/*ipaddr*

# 8. Create inventory.txt listing all processed files
find data/processed -type f > data/inventory.txt

###########################################

echo "Project setup is complete!"
