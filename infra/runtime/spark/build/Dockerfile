# The Spark version must be compatible with the Hadoop and AWS SDK versions
# provided. 
ARG SPARK_VERSION=v3.2.2
# For Hadoop, a good way to figure out the version is to check
# existing Hadoop related dependencies in the /opt/spark/jars dir of the 
# Spark image.
ARG HADOOP_VERSION=3.3.1
# There is no clear rule for the SDK version required by Hadoop and Spark
ARG AWS_JAVA_SDK_VERSION=1.12.302
# The debian version used to get the Lambdaric should ideally match the one 
# in the Spark image (cat /etc/debian_version)

ARG FUNCTION_DIR="/function"

FROM curlimages/curl as jar-dependencies
ARG AWS_JAVA_SDK_VERSION
ARG HADOOP_VERSION

RUN curl https://repo1.maven.org/maven2/com/amazonaws/aws-java-sdk-core/$AWS_JAVA_SDK_VERSION/aws-java-sdk-core-$AWS_JAVA_SDK_VERSION.jar -o /tmp/aws-java-sdk-core-$AWS_JAVA_SDK_VERSION.jar
RUN curl https://repo1.maven.org/maven2/com/amazonaws/aws-java-sdk-s3/$AWS_JAVA_SDK_VERSION/aws-java-sdk-s3-$AWS_JAVA_SDK_VERSION.jar -o /tmp/aws-java-sdk-s3-$AWS_JAVA_SDK_VERSION.jar
RUN curl https://repo1.maven.org/maven2/com/amazonaws/aws-java-sdk-dynamodb/$AWS_JAVA_SDK_VERSION/aws-java-sdk-dynamodb-$AWS_JAVA_SDK_VERSION.jar -o /tmp/aws-java-sdk-dynamodb-$AWS_JAVA_SDK_VERSION.jar
RUN curl https://repo1.maven.org/maven2/com/amazonaws/aws-java-sdk/$AWS_JAVA_SDK_VERSION/aws-java-sdk-$AWS_JAVA_SDK_VERSION.jar -o /tmp/aws-java-sdk-$AWS_JAVA_SDK_VERSION.jar
RUN curl https://repo1.maven.org/maven2/org/apache/hadoop/hadoop-common/$HADOOP_VERSION/hadoop-common-$HADOOP_VERSION.jar -o /tmp/hadoop-common-$HADOOP_VERSION.jar
RUN curl https://repo1.maven.org/maven2/org/apache/hadoop/hadoop-aws/$HADOOP_VERSION/hadoop-aws-$HADOOP_VERSION.jar -o /tmp/hadoop-aws-$HADOOP_VERSION.jar


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
    awslambdaric
# Copy function code
COPY lambda-handler.py ${FUNCTION_DIR}


FROM apache/spark:$SPARK_VERSION
ARG AWS_JAVA_SDK_VERSION
ARG HADOOP_VERSION
ARG FUNCTION_DIR

COPY --from=jar-dependencies /tmp/aws-java-sdk-core-$AWS_JAVA_SDK_VERSION.jar /opt/spark/jars/aws-java-sdk-core-$AWS_JAVA_SDK_VERSION.jar
COPY --from=jar-dependencies /tmp/aws-java-sdk-s3-$AWS_JAVA_SDK_VERSION.jar /opt/spark/jars/aws-java-sdk-s3-$AWS_JAVA_SDK_VERSION.jar
COPY --from=jar-dependencies /tmp/aws-java-sdk-dynamodb-$AWS_JAVA_SDK_VERSION.jar /opt/spark/jars/aws-java-sdk-dynamodb-$AWS_JAVA_SDK_VERSION.jar
COPY --from=jar-dependencies /tmp/aws-java-sdk-$AWS_JAVA_SDK_VERSION.jar /opt/spark/jars/aws-java-sdk-$AWS_JAVA_SDK_VERSION.jar
COPY --from=jar-dependencies /tmp/hadoop-common-$HADOOP_VERSION.jar /opt/spark/jars/hadoop-common-$HADOOP_VERSION.jar
COPY --from=jar-dependencies /tmp/hadoop-aws-$HADOOP_VERSION.jar /opt/spark/jars/hadoop-aws-$HADOOP_VERSION.jar

USER root

RUN apt update && \
    apt install -y python3 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=ric-dependency ${FUNCTION_DIR} ${FUNCTION_DIR}
COPY ./spark-class /opt/spark/bin/
COPY ./spark-defaults.conf /opt/spark/conf/

WORKDIR ${FUNCTION_DIR}

ENTRYPOINT [ "python3", "-m", "awslambdaric" ]
CMD [ "lambda-handler.handler" ]
