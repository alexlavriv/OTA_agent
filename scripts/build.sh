#!/bin/bash
set -e

CI_COMMIT_REF_NAME=$1
CI_COMMIT_SHORT_SHA=$2

/usr/local/bin/jfrog c add phantom --artifactory-url="${ARTIFACTORY_URL}" --user="${ARTIFACTORY_USER}" --password="${ARTIFACTORY_PASS}" --interactive=false
/usr/local/bin/jfrog c show


echo CI_COMMIT_REF_NAME: "$CI_COMMIT_REF_NAME"
echo CI_COMMIT_SHORT_SHA: "$CI_COMMIT_SHORT_SHA"
source "$HOME"/.cargo/env
cargo --version
cargo clean
export CI_COMMIT_REF_NAME="$CI_COMMIT_REF_NAME"
export CI_COMMIT_SHORT_SHA="$CI_COMMIT_SHORT_SHA"
cargo build --release --verbose
cargo install --force cargo-strip
cargo-strip

