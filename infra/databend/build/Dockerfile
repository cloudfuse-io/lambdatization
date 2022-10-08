
ARG DATABEND_VERSION=v0.8.61-nightly
# for v0.8.61-nightly, the experimental_allow_proto3_optional is used
# which is only available for protoc >=3.12 and exited experimental mode
# in 3.15 (the `experimental_` prefix will stop working at some point)
ARG PROTOC_VERSION=3.15.8
ARG FUNCTION_DIR="/function"

FROM rust:1.64-bullseye as build
ARG DATABEND_VERSION
ARG PROTOC_VERSION

RUN PB_REL="https://github.com/protocolbuffers/protobuf/releases" && \
    curl -LO $PB_REL/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-x86_64.zip && \
    unzip protoc-${PROTOC_VERSION}-linux-x86_64.zip -d /usr/local/

RUN git clone --depth 1 --branch ${DATABEND_VERSION} https://github.com/datafuselabs/databend.git 
WORKDIR /databend
RUN echo protoc -v
RUN cargo build --bin=databend-query --bin=databend-meta --bin=databend-metactl --release


FROM ubuntu:22.04 as ric-dependency

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
# Install the runtime interface client
RUN pip3 install \
    --target ${FUNCTION_DIR} \
    awslambdaric \
    requests
# Copy function code
COPY lambda-handler.py ${FUNCTION_DIR}


FROM ubuntu:22.04
ARG FUNCTION_DIR

RUN apt-get update -y && \
    apt-get install -y apt-transport-https ca-certificates python3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /var/cache/apt/*

COPY --from=build /databend/target/release/databend-query /databend-query
COPY --from=build /databend/target/release/databend-meta /databend-meta
COPY ./bootstrap.sh /bootstrap.sh
COPY ./databend-query.toml /databend-query.toml
COPY ./databend-meta.toml /databend-meta.toml
COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
