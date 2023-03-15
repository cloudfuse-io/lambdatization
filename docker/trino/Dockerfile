ARG FUNCTION_DIR="/function"
ARG HADOOP_VERSION=3.2.0
# The SDK version must be the one in the Hadoop package
ARG AWS_JAVA_SDK_VERSION=1.11.375
ARG METASTORE_VERSION=3.0.0
# We use custom builds of trino-server
ARG TRINO_VERSION=378


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
ARG TRINO_VERSION
ARG AWS_JAVA_SDK_VERSION

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
# jars used by Trino
ENV HADOOP_CLASSPATH=${HADOOP_HOME}/share/hadoop/tools/lib/aws-java-sdk-bundle-${AWS_JAVA_SDK_VERSION}.jar:${HADOOP_HOME}/share/hadoop/tools/lib/hadoop-aws-${HADOOP_VERSION}.jar

RUN curl -L https://repo1.maven.org/maven2/org/apache/hive/hive-standalone-metastore/${METASTORE_VERSION}/hive-standalone-metastore-${METASTORE_VERSION}-bin.tar.gz | tar zxf - && \
  # Download from mirror and trim some unused libraries
  curl -L https://github.com/cloudfuse-io/lambdatization/releases/download/hadoop-${HADOOP_VERSION}/hadoop-${HADOOP_VERSION}.tar.gz | tar zxf - && \
  cd hadoop-${HADOOP_VERSION}/share/hadoop/ && \
  rm -r client/* && \
  rm -r hdfs/* && \
  rm -r mapreduce/* && \
  rm -r yarn/* && \
  find ./tools/lib -type f -not \( -name "aws-java-sdk-bundle-${AWS_JAVA_SDK_VERSION}.jar" -or -name "hadoop-aws-${HADOOP_VERSION}.jar" \) -delete

ENV PATH="${HIVE_HOME}/bin:${PATH}"
COPY metastore-site.xml ${HIVE_HOME}/conf

# TRINO

ENV TRINO_HOME=/opt/trino-server-${TRINO_VERSION}
RUN curl -L https://github.com/cloudfuse-io/lambdatization/releases/download/trino-server-${TRINO_VERSION}/trino-server-${TRINO_VERSION}.tar.gz | tar zxf - && \
  curl -L https://repo1.maven.org/maven2/io/trino/trino-cli/${TRINO_VERSION}/trino-cli-${TRINO_VERSION}-executable.jar -o ${TRINO_HOME}/bin/trino && \
  chmod +x ${TRINO_HOME}/bin/trino
ENV PATH="${TRINO_HOME}/bin:${PATH}"
COPY trino-etc ${TRINO_HOME}/etc

# LAMBDA ENTRYPOINT

ARG FUNCTION_DIR
COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
WORKDIR ${FUNCTION_DIR}
ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
