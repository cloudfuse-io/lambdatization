ARG DREMIO_VERSION=22.1
ARG FUNCTION_DIR="/function"

FROM debian:11.4 as ric-dependency
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
    awslambdaric requests
COPY lambda-handler.py ${FUNCTION_DIR}


FROM dremio/dremio-oss:$DREMIO_VERSION

ARG FUNCTION_DIR

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
COPY ./dremio.conf /opt/dremio/conf/
COPY ./dremio-env /opt/dremio/conf/

USER root
RUN  mkdir -p /var/lib/dremio/dremio/.aws && ln -s /tmp/aws/credentials /var/lib/dremio/dremio/.aws/credentials
RUN  mkdir -p /home/sbx_user1051/.aws && ln -s /tmp/aws/credentials /home/sbx_user1051/.aws/credentials
USER dremio

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
