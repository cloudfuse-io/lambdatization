
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

# Install the runtime interface client and lambda requirements
RUN pip3 install \
    --target ${FUNCTION_DIR} \
    awslambdaric pexpect

#ballista-builder.Dockerfile
FROM rust:1.63.0-buster as builder

# Include global arg in this stage of the build
ARG RELEASE_FLAG

ENV RUST_LOG=error
ENV RUST_BACKTRACE=full
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get -y install libssl-dev openssl zlib1g zlib1g-dev libpq-dev cmake protobuf-compiler netcat curl unzip git

# create build directories
RUN mkdir -p /home/builder/workspace

ENV HOME=/home/builder
ENV PATH=$HOME/.cargo/bin:$PATH

# prepare rust
RUN rustup update && \
    rustup component add rustfmt && \
    cargo install cargo-chef --version 0.1.34

WORKDIR /home/builder/workspace/

RUN git clone https://github.com/apache/arrow-ballista.git

WORKDIR /home/builder/workspace/arrow-ballista

RUN ls -l ./

# Actually build the project
RUN cargo build --features "flight-sql s3" --${RELEASE_FLAG} "$@"


FROM ubuntu:22.04

# Include global arg in this stage of the build
ARG RELEASE_FLAG
ARG FUNCTION_DIR

ENV RELEASE_FLAG=${RELEASE_FLAG}
ENV RUST_LOG=error
ENV RUST_BACKTRACE=full
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y netcat python3 

# copy built scheduler into accesible folder
COPY --from=builder /home/builder/workspace/arrow-ballista/target/$RELEASE_FLAG/ballista-scheduler /opt/ballista/ballista-scheduler

# Expose Ballista Scheduler gRPC port
EXPOSE 50050

# copy built executor into accesible folder
COPY --from=builder /home/builder/workspace/arrow-ballista/target/$RELEASE_FLAG/ballista-executor /opt/ballista/ballista-executor

# Expose Ballista Executor gRPC port
EXPOSE 50051

# copy builtcli into accesible folder
COPY --from=builder /home/builder/workspace/arrow-ballista/target/$RELEASE_FLAG/ballista-cli /opt/ballista/ballista-cli

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
