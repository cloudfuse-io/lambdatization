
ARG CLICKHOUSE_VERSION=22.10.2.11
ARG FUNCTION_DIR="/function"


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
COPY lambda-handler.py ${FUNCTION_DIR}


FROM ghcr.io/cloudfuse-io/lambdatization:v$CLICKHOUSE_VERSION-patch
ARG FUNCTION_DIR

RUN apt-get update -y && \
    apt-get install -y python3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /var/cache/apt/*

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
COPY ./config.xml /etc/clickhouse-server/config.xml
RUN rm /etc/clickhouse-server/config.d/docker_related_config.xml

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
