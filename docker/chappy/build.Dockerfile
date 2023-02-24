FROM rust:buster as build
ARG BUILD_FLAG=""

RUN apt update && apt install -y protobuf-compiler

RUN mkdir /code

WORKDIR /code
COPY . .

RUN --mount=type=cache,target=./target \
  --mount=type=cache,target=/usr/local/cargo/git \
  --mount=type=cache,target=/usr/local/cargo/registry \
  cargo build ${BUILD_FLAG} && \
  cp -r ./target /target
