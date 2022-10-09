
ARG DATABEND_VERSION=v0.8.61-nightly
ARG FUNCTION_DIR="/function"


FROM ubuntu:22.04 as download
ARG DATABEND_VERSION
RUN apt update && apt install -y curl
RUN DB_REL="https://github.com/datafuselabs/databend/releases" && \
    curl -LO $DB_REL/download/${DATABEND_VERSION}/databend-${DATABEND_VERSION}-x86_64-unknown-linux-gnu.tar.gz
RUN mkdir -p /databend/target/release/ && \
    tar -xvf databend-${DATABEND_VERSION}-x86_64-unknown-linux-gnu.tar.gz -C /databend/target/release/ --strip-components=1 bin

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

COPY --from=download /databend/target/release/databend-query /databend-query
COPY --from=download /databend/target/release/databend-meta /databend-meta
COPY ./bootstrap.sh /bootstrap.sh
COPY ./databend-query.toml /databend-query.toml
COPY ./databend-meta.toml /databend-meta.toml
COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
