.PHONY : all
.DEFAULT_GOAL := build
.EXPORT_ALL_VARIABLES:

CI_COMMIT_REF_NAME ?= $(shell git rev-parse --abbrev-ref HEAD)
LATEST_TAG := $(shell git describe --tags `git rev-list --tags --max-count=1`)
ARCH := $(shell uname -p)
SHELL := /bin/bash


build_gdb_ssh:
	docker buildx build -f docker/Dockerfile_GDB_SSH -t phau/coreagent:local-dev-gdb-ssh .

run_gdb_ssh:
	IMAGE_TAG=local-dev-gdb-ssh docker-compose -f ./docker/agent.yaml up --force-recreate --remove-orphans

build:
	docker buildx build -f docker/Dockerfile -t phau/coreagent:local-dev .

