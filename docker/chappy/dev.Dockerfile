
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

RUN apt-get update && \
    apt-get install -y alien && \
    curl -O https://lambda-insights-extension.s3-ap-northeast-1.amazonaws.com/amazon_linux/lambda-insights-extension.rpm && \
    alien --to-deb lambda-insights-extension.rpm -i && \
    rm -f lambda-insights-extension.rpm && \
    apt-get remove -y alien && \
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
