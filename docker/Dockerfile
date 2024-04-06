FROM phusion/baseimage:18.04-1.0.0

ARG AWS_ID
ARG AWS_KEY
ENV AWS_ACCESS_KEY_ID=$AWS_ID
ENV AWS_SECRET_ACCESS_KEY=$AWS_KEY

COPY ./scripts/install_build_dependencies.sh .
RUN chmod +x install_build_dependencies.sh && ./install_build_dependencies.sh

# Configure Artifactory instance with JFrog CLI
WORKDIR /
ARG ARTIFACTORY_USER
ARG ARTIFACTORY_PASS
ARG ARTIFACTORY_URL
ARG CI_COMMIT_REF_NAME
ARG CI_COMMIT_SHORT_SHA
RUN echo "alex"
RUN echo CI_COMMIT_REF_NAME: $CI_COMMIT_REF_NAME
RUN echo CI_COMMIT_SHORT_SHA: $CI_COMMIT_SHORT_SHA
RUN curl -fL https://getcli.jfrog.io | bash -s v2
RUN chmod +x ./jfrog && mkdir -p /usr/local/bin && mv ./jfrog /usr/local/bin/jfrog

# Install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y

WORKDIR /agent
COPY ./scripts/build.sh .
COPY ./src ./src
COPY ./Cargo.toml .

RUN chmod +x build.sh && ./build.sh $CI_COMMIT_REF_NAME  $CI_COMMIT_SHORT_SHA

WORKDIR /

#  cleanup
ARG ARCH
ENV AGENT_ARTIFACTS_FILE_NAME="sdk_coreagent_artifacts"
ARG REF
COPY ./scripts/collect_and_upload_artifacts.sh .
RUN chmod +x collect_and_upload_artifacts.sh && ./collect_and_upload_artifacts.sh ${AGENT_ARTIFACTS_FILE_NAME} ${ARCH} ${REF}

RUN apt-get autoremove -y && apt-get clean
