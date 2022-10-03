
ARG DREMIO_VERSION=22.1


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

# Install the runtime interface client
RUN pip3 install \
    --target ${FUNCTION_DIR} \
    awslambdaric requests

# Copy function code
COPY lambda-handler.py ${FUNCTION_DIR}


FROM dremio/dremio-oss:$DREMIO_VERSION

ARG FUNCTION_DIR

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
COPY ./dremio.conf /opt/dremio/conf/
COPY ./dremio-env /opt/dremio/conf/
ENV AWS_CREDENTIAL_PROFILES_FILE=/tmp/aws/credentials

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
