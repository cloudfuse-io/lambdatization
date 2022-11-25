ARG IMAGE_SIZE_MB
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


FROM ubuntu:20.04

ENV DEBIAN_FRONTEND=noninteractive

ARG FUNCTION_DIR
ARG IMAGE_SIZE_MB

RUN apt-get update \
  && apt-get install -y python3 \
  && rm -rf /var/lib/apt/lists/*

RUN head -c ${IMAGE_SIZE_MB}MB /dev/urandom > /placeholder.bin

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
