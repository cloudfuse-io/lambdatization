
ARG BALLISTA_VERSION=0.8.0
ARG RELEASE_FLAG=release
ARG FUNCTION_DIR="/function"

FROM debian:11.4 as ric-dependency

# Install aws-lambda-cpp build dependencies
RUN apt-get update && \
    apt-get install -y \
    g++ \
    make \
    cmake \
    unzip \
    python3 \
    python3-pip \
    libcurl4-openssl-dev

# Include global arg in this stage of the build
ARG FUNCTION_DIR
# Create function directory
RUN mkdir -p ${FUNCTION_DIR}

# Copy function code
COPY lambda-handler.py ${FUNCTION_DIR}

# Install the runtime interface client
RUN pip3 install \
    --target ${FUNCTION_DIR} \
    awslambdaric

#ballista-builder.Dockerfile
FROM rust:1.63.0-buster

ARG EXT_UID
# Include global arg in this stage of the build
ARG RELEASE_FLAG

ENV RUST_LOG=info
ENV RUST_BACKTRACE=full
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get -y install libssl-dev openssl zlib1g zlib1g-dev libpq-dev cmake protobuf-compiler netcat curl unzip

# create build user with same UID as 
RUN adduser -q -u $EXT_UID builder --home /home/builder && \
    mkdir -p /home/builder/workspace
USER builder

ENV HOME=/home/builder
ENV PATH=$HOME/.cargo/bin:$PATH

# prepare rust
RUN rustup update && \
    rustup component add rustfmt && \
    cargo install cargo-chef --version 0.1.34

#get ballista
RUN curl -L https://github.com/apache/arrow-ballista/archive/refs/tags/${BALLISTA_VERSION}.tar.gz  | tar -xzf - -C /home/builder/workspace

WORKDIR /home/builder/workspace

# COPY dev/docker/builder-entrypoint.sh /home/builder
# ENTRYPOINT ["/home/builder/builder-entrypoint.sh"]
RUN cargo build --features "flight-sql s3" --${RELEASE_FLAG} "$@"


# ballista-scheduler.Dockerfile
FROM ubuntu:22.04

# Include global arg in this stage of the build
ARG RELEASE_FLAG

ENV RELEASE_FLAG=${RELEASE_FLAG}
ENV RUST_LOG=info
ENV RUST_BACKTRACE=full
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y netcat

COPY target/$RELEASE_FLAG/ballista-scheduler /root/ballista-scheduler

# Expose Ballista Scheduler gRPC port
EXPOSE 50050

# COPY dev/docker/scheduler-entrypoint.sh /root/scheduler-entrypoint.sh
# ENTRYPOINT ["/root/scheduler-entrypoint.sh"]
# RUN /root/ballista-scheduler "$@" -> moved to python


# ballista-executor.Dockerfile
COPY target/$RELEASE_FLAG/ballista-executor /root/ballista-executor

# Expose Ballista Executor gRPC port
EXPOSE 50051

# COPY dev/docker/executor-entrypoint.sh /root/executor-entrypoint.sh
# ENTRYPOINT ["/root/executor-entrypoint.sh"]
# RUN /root/ballista-executor "$@" -> moved to python


USER root

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
