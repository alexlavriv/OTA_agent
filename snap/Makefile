.PHONY : all backoffice backoffice_qa backoffice_prod
default: build;

LINK ?= http://example.com
TOKEN ?= default_token

CURRENT_DIR := $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))
SHELL := /bin/bash
SNAPCRAFT_YAML := $(CURRENT_DIR)/snap/snapcraft.yaml
TAG_REGEX := ^([0-9]{1,}\.){2}[0-9]{1,}$

ARCHIE :=
ifeq ($(ARCH),)
	architecture := $(shell uname -p)
	ifeq ($(architecture),x86_64)
		ARCHIE := amd64
	else ifeq ($(architecture),aarch64)
		ARCHIE := arm64
	else
		(echo "Unsupported architecture: $(architecture)"; exit 1)
	endif
else
	ARCHIE := $(ARCH)
endif

REF :=
ifeq ($(CI_COMMIT_REF_NAME),) ##local build
	REF := $(shell git describe --tags `git rev-list --tags --max-count=1`)
 	CI_COMMIT_REF_NAME := $(REF)
else
	# When building a SNAP, the version must be something that is supported. Since we often build in branches, just use the latest tag as the version
	ifeq ($(shell echo ${CI_COMMIT_REF_NAME} | egrep "${TAG_REGEX}"),)
		REF := $(shell git describe --tags `git rev-list --tags --max-count=1`)
	else
		REF := $(CI_COMMIT_REF_NAME)
	endif
endif

# place your branch name here for local builds
BRANCH_NAME :=
AGENT_ARTIFACTS_FILE := sdk_coreagent_artifacts.tar.gz

download:
	./jfrog rt dl /Phantom.Binary/SDK-Phantom-Agent/$(CI_COMMIT_REF_NAME)/$(ARCHIE)/$(AGENT_ARTIFACTS_FILE) --flat

expand:
	mkdir -p rootfs 2>/dev/null
	tar -xvf $(AGENT_ARTIFACTS_FILE) -C rootfs

build:
	-rm -f $(SNAPCRAFT_YAML)
	-mkdir -p $(CURRENT_DIR)/snap

	pip3 install -r ./requirements.txt
	./generate_yaml.py --arch $(ARCHIE) --version $(REF)
	sudo snapcraft --debug --use-lxd

install:
	sudo snap install ./phantom-agent_$(REF)_$(ARCHIE).snap --dangerous --classic

upload:
	./jfrog rt u ./phantom-agent_$(REF)_$(ARCHIE).snap Phantom.Binary/SDK-Phantom-Agent/$(CI_COMMIT_REF_NAME)/$(ARCHIE)/
	aws s3 cp ./phantom-agent_$(REF)_$(ARCHIE).snap s3://phau-artifactory-eng2/Phantom.Binary/SDK-Phantom-Agent/$(CI_COMMIT_REF_NAME)/$(ARCHIE)/
	
SNAP_FILE_SHA :=
UPPER_ARCH := $(shell echo $(ARCHIE) | tr a-z A-Z)
ifneq ("$(wildcard ./phantom-agent_$(REF)_$(ARCHIE).snap)","")
	SNAP_FILE_SHA := $(shell sha1sum ./phantom-agent_$(REF)_$(ARCHIE).snap | awk '{print $$1}')
endif

download_snap:
ifeq ($(SNAP_FILE_SHA),)
	./jfrog rt dl Phantom.Binary/SDK-Phantom-Agent/$(CI_COMMIT_REF_NAME)/$(ARCHIE)/phantom-agent_$(REF)_$(ARCHIE).snap --flat
else
	@echo "Snap file already exists"
endif



define backoffice_register
	@echo "Link: $(LINK)"
	@CURL_RES=$(shell curl -X POST "$(1)" -w "%{http_code}" -o output.json -H "accept: application/json" -H  "Authorization: $(2)" -H "Content-Type: application/json" -d "{\"component\":\"phantom_agent\",\"version\":\"$(REF)\",\"link\":\"https://phantomauto.jfrog.io/artifactory/Phantom.Binary/SDK-Phantom-Agent/$(CI_COMMIT_REF_NAME)/$(ARCHIE)/phantom-agent_$(REF)_$(ARCHIE).snap\",\"checksum\":\"$(SNAP_FILE_SHA)\",\"arch\":\"$(UPPER_ARCH)\"}" | awk '{print int($$1/100)}'); \
	if [ $$CURL_RES != 2 ]; then \
		echo "Failed to register tag. ERROR:"; \
		ERR_MSG=$$(cat output.json | jq ".message"); \
		echo $$ERR_MSG; \
	fi

endef


backoffice:
ifneq ($(SNAP_FILE_SHA),)
	$(call backoffice_register,$(LINK),$(TOKEN))
else
	@echo "SNAP file does not exist"
	exit 1
endif

clean:
	-sudo snap remove phantom-agent
	-sudo lxc delete snapcraft-phantom-agent --force
	-rm -rf rootfs || true
	-rm -rf $(AGENT_ARTIFACTS_FILE) || true
	-rm $(SNAPCRAFT_YAML) || true
	-rm -rf ./snap || true
	-rm -rf ./phantom-agent_$(REF)_$(ARCHIE).snap || true
