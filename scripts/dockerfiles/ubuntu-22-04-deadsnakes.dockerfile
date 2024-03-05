FROM ubuntu:22.04
ENV DEBIAN_FRONTEND noninteractive
RUN apt update
RUN apt install -yy software-properties-common
RUN add-apt-repository ppa:deadsnakes/ppa
RUN apt update
RUN apt install -yy python3.12 python3.12-dev python3.11 python3.11-dev python3.10 python3.10-dev python3.9 python3.9-dev python3.8 python3.8-dev build-essential

