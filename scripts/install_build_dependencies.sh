#!/bin/bash

# exit when any command fails
set -xe

apt update && apt install --no-install-recommends -y \
  build-essential \
  awscli \
  curl \
  gcc \
  libssl-dev \
  libsystemd-dev \
  make \
  pkg-config \
  python3-pip \
  python3-setuptools\
  rsync \
  && ldconfig
