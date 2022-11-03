ARG FUNCTION_DIR="/function"
ARG HADOOP_VERSION=3.2.0
ARG METASTORE_VERSION=3.0.0


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
ARG HADOOP_VERSION
ARG METASTORE_VERSION

ENV DEBIAN_FRONTEND=noninteractive 

RUN apt-get update && apt-get install -y \
  curl \
  less \
  openjdk-11-jdk \
  python3 \
  && rm -rf /var/lib/apt/lists/*
ENV JAVA_HOME=/usr/lib/jvm/java-11-openjdk-amd64/
RUN ln -s /usr/bin/python3 /usr/bin/python

# HIVE METASTORE

WORKDIR /opt

ENV HADOOP_HOME=/opt/hadoop-${HADOOP_VERSION}
ENV HIVE_HOME=/opt/apache-hive-metastore-${METASTORE_VERSION}-bin
ENV HADOOP_CLASSPATH=${HADOOP_HOME}/share/hadoop/tools/lib/aws-java-sdk-bundle-1.11.375.jar:${HADOOP_HOME}/share/hadoop/tools/lib/hadoop-aws-3.2.0.jar

RUN curl -L https://archive.apache.org/dist/hive/hive-standalone-metastore-${METASTORE_VERSION}/hive-standalone-metastore-${METASTORE_VERSION}-bin.tar.gz | tar zxf - && \
  curl -L https://archive.apache.org/dist/hadoop/common/hadoop-${HADOOP_VERSION}/hadoop-${HADOOP_VERSION}.tar.gz | tar zxf -
ENV PATH="${HIVE_HOME}/bin:${PATH}"
COPY metastore-site.xml ${HIVE_HOME}/conf

# TRINO

ENV TRINO_VERSION=378
ENV TRINO_HOME=/opt/trino-server-${TRINO_VERSION}
RUN curl -L https://repo1.maven.org/maven2/io/trino/trino-server/${TRINO_VERSION}/trino-server-${TRINO_VERSION}.tar.gz | tar zxf -
RUN curl -L https://repo1.maven.org/maven2/io/trino/trino-cli/${TRINO_VERSION}/trino-cli-${TRINO_VERSION}-executable.jar -o ${TRINO_HOME}/bin/trino && \
  chmod +x ${TRINO_HOME}/bin/trino
ENV PATH="${TRINO_HOME}/bin:${PATH}"
COPY trino-etc ${TRINO_HOME}/etc

# LAMBDA ENTRYPOINT

ARG FUNCTION_DIR
COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
WORKDIR ${FUNCTION_DIR}
ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
