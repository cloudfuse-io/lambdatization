
ARG BALLISTA_VERSION=0.11.0
ARG RELEASE_FLAG=release
ARG FUNCTION_DIR="/function"


FROM curlimages/curl as bin-dependencies
ARG BALLISTA_VERSION

RUN curl -L https://github.com/cloudfuse-io/lambdatization/releases/download/ballista-${BALLISTA_VERSION}/ballista-scheduler -o /tmp/ballista-scheduler
RUN curl -L https://github.com/cloudfuse-io/lambdatization/releases/download/ballista-${BALLISTA_VERSION}/ballista-executor -o /tmp/ballista-executor
RUN curl -L https://github.com/cloudfuse-io/lambdatization/releases/download/ballista-${BALLISTA_VERSION}/ballista-cli -o /tmp/ballista-cli


FROM python:3.10-bullseye as ric-dependency

ARG FUNCTION_DIR
ENV DEBIAN_FRONTEND=noninteractive

# Install aws-lambda-cpp build dependencies
RUN apt-get update && \
    apt-get -y install\
    g++ \
    make \
    cmake \
    unzip \
    netcat \
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
    awslambdaric


FROM python:3.10-slim-bullseye
ARG FUNCTION_DIR

ENV RUST_LOG=warn
ENV RUST_BACKTRACE=full

COPY --from=bin-dependencies /tmp/ballista-scheduler /opt/ballista/ballista-scheduler
COPY --from=bin-dependencies /tmp/ballista-executor /opt/ballista/ballista-executor
COPY --from=bin-dependencies /tmp/ballista-cli /opt/ballista/ballista-cli

RUN chmod +x /opt/ballista/ballista-scheduler  && \
  chmod +x /opt/ballista/ballista-executor  && \
  chmod +x /opt/ballista/ballista-cli

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
