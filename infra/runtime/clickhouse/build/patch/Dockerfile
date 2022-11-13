## Helper image to build clickhouse

FROM ubuntu:20.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y \
        git \
        ccache \
        python3 \
        ninja-build \
        wget \
        lsb-release \
        software-properties-common \
        gnupg

# Install recent version of CMake (3.20 required)
RUN wget https://github.com/Kitware/CMake/releases/download/v3.24.3/cmake-3.24.3-linux-x86_64.sh && \
    chmod +x cmake-3.24.3-linux-x86_64.sh && \
    mkdir -p /opt/cmake && \
    ./cmake-3.24.3-linux-x86_64.sh --skip-license --prefix=/opt/cmake && \
    rm cmake-3.24.3-linux-x86_64.sh

ENV PATH=$PATH:/opt/cmake/bin/

RUN wget https://apt.llvm.org/llvm.sh && \
    chmod +x llvm.sh && \
    ./llvm.sh 15 all && \
    rm llvm.sh

ENV CC=clang-15
ENV CXX=clang++-15
