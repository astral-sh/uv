# Use bullseye
FROM debian:bullseye

RUN apt-get update
RUN apt-get install python3 python3-pip python3-venv -y
