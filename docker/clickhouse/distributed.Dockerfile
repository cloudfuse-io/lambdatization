
ARG CLICKHOUSE_VERSION=22.10.2.11
ARG FUNCTION_DIR="/function"

FROM rust:bullseye as chappy-build
RUN apt update && apt install -y protobuf-compiler
RUN mkdir /code
WORKDIR /code
COPY chappy/ .
RUN cargo build --release


FROM ubuntu:20.04 as ric-dependency

ENV DEBIAN_FRONTEND=noninteractive 

RUN apt-get update && \
    apt-get install -y \
    g++ \
    make \
    cmake \
    unzip \
    python3 \
    python3-pip \
    libcurl4-openssl-dev
ARG FUNCTION_DIR
RUN mkdir -p ${FUNCTION_DIR}
RUN pip3 install \
    --target ${FUNCTION_DIR} \
    awslambdaric
COPY docker/clickhouse/distributed-handler.py ${FUNCTION_DIR}/lambda-handler.py


FROM ghcr.io/cloudfuse-io/lambdatization:clickhouse-v$CLICKHOUSE_VERSION-patch
ARG FUNCTION_DIR

ENV RUST_LOG=warn
ENV RUST_BACKTRACE=full
ENV LD_PRELOAD=/opt/ballista/libchappy.so

COPY --from=chappy-build /code/target/release/libchappy.so /opt/ballista/libchappy.so
COPY --from=chappy-build /code/target/release/chappy-perforator /opt/ballista/chappy-perforator

RUN apt-get update -y && \
    apt-get install -y python3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /var/cache/apt/*

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
COPY docker/clickhouse/distributed-config.xml /etc/clickhouse-server/config.xml
ENV CLICKHOUSE_WATCHDOG_ENABLE=0
RUN rm /etc/clickhouse-server/config.d/docker_related_config.xml

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
