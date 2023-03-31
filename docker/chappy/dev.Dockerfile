
ARG FUNCTION_DIR="/function"

FROM python:3.10-bullseye
RUN apt-get update && \
    apt-get install -y \
        g++ \
        make \
        cmake \
        unzip \
        apt-transport-https \
        ca-certificates \
        libcurl4-openssl-dev && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /var/cache/apt/*

ARG FUNCTION_DIR

RUN mkdir -p ${FUNCTION_DIR}

RUN pip3 install \
    --target ${FUNCTION_DIR} \
    awslambdaric \
    boto3

WORKDIR ${FUNCTION_DIR}
COPY dev-handler.py .

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "dev-handler.handler" ]
