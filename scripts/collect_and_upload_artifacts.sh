#!/bin/bash
pushd .
FILE_NAME=$1
ARCH=$2
REF=$3
TMP_DIR=$(mktemp -d)

function usage() {
  cat <<EOU
    Usage: bash $0 <Core Artifacts File Name> <Architecture>
EOU
  exit 1
}

# shellcheck disable=SC2082
if [ -z "${FILE_NAME}" ] || [ -z "${ARCH}" ] || [ -z "${REF}" ]; then
  echo "Error: One or more missing arguments!"
  echo "FILE_NAME=${FILE_NAME}"
  echo "ARCH=${ARCH}"
  echo "REF=${REF}"
  usage
fi

AGENT_ARTIFACTS_FILE_NAME="${FILE_NAME}"

echo "Collecting artifacts..."
ARTIFACTS=(
  "/agent/target/release/phantom_agent"
)

for ARTIFACT in "${ARTIFACTS[@]}"; do
  cp -R "${ARTIFACT}" "${TMP_DIR}"
done

echo "Building archive and uploading..."
tar -czf "${AGENT_ARTIFACTS_FILE_NAME}".tar.gz -C "${TMP_DIR}" .
/usr/local/bin/jfrog rt u "${AGENT_ARTIFACTS_FILE_NAME}".tar.gz /Phantom.Binary/SDK-Phantom-Agent/"$REF"/"$ARCH"/
aws s3 cp "${AGENT_ARTIFACTS_FILE_NAME}".tar.gz s3://phau-artifactory-eng2/Phantom.Binary/SDK-Phantom-Agent/"$REF"/"$ARCH"/
rm "${AGENT_ARTIFACTS_FILE_NAME}".tar.gz
rm -rf "${TMP_DIR}"

echo "Done!"
popd || exit
