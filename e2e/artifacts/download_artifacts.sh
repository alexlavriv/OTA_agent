#!/bin/bash
JFROG_USER=alex.lavriv
JFROG_PASSWORD=4Ce6bcmU4MpzCaU
ARCH=amd64
SNAP_EXTENSION=snap

PHANTOM_AGENT_VERSION=0.6.14
PHANTOM_AGENT_BRANCH=0.6.14
PHANTOM_AGENT_REPO=SDK-Phantom-Agent
PHANTOM_AGENT_FILE_NAME=phantom-agent

PHANTOM_CORE_VERSION=3.0.25
PHANTOM_CORE_BRANCH=3.0.25
PHANTOM_CORE_REPO=Core
PHANTOM_CORE_FILE_NAME=phau-core

#Phantom.Binary/SDK-Phantom-Agent/dev--11235--alex/amd64/phantom-agent_0.0.14_amd64.snap
#https://phantomauto.jfrog.io/artifactory/Phantom.Binary/Core/3.0.9/amd64/phau-core_3.0.9_amd64.snap

echo Downloading artifacts
download_jfrog() {
    ARCH=$1
    VERSION=$2
    REPO=$3
    BRANCH=$4
    FILE_NAME=$5
    EXTENSION=$6
    echo ARCH: "$ARCH"
    echo VERSION: "$VERSION"
    echo REPO: "$REPO"
    echo FILE_NAME: "$FILE_NAME"
    echo EXTENSION: "$EXTENSION"
    echo jfrog rt dl --flat Phantom.Binary/"$REPO"/"$BRANCH"/"$ARCH"/"$FILE_NAME"_"$VERSION"_"$ARCH"."$EXTENSION" "$FILE_NAME"."$EXTENSION"
    jfrog rt dl --flat Phantom.Binary/"$REPO"/"$BRANCH"/"$ARCH"/"$FILE_NAME"_"$VERSION"_"$ARCH"."$EXTENSION" "$FILE_NAME"."$EXTENSION"

}
jfrog c add --artifactory-url "https://phantomauto.jfrog.io/artifactory" --user "$JFROG_USER" --password="$JFROG_PASSWORD" --interactive=false
download_jfrog "$ARCH" "$PHANTOM_AGENT_VERSION" "$PHANTOM_AGENT_REPO" "$PHANTOM_AGENT_BRANCH" "$PHANTOM_AGENT_FILE_NAME" "$SNAP_EXTENSION"
download_jfrog "$ARCH" "$PHANTOM_CORE_VERSION" "$PHANTOM_CORE_REPO" "$PHANTOM_CORE_BRANCH" "$PHANTOM_CORE_FILE_NAME" "$SNAP_EXTENSION"

