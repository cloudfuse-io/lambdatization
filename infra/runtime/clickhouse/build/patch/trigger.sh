#!/bin/bash
set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

pushd $SCRIPT_DIR

[ ! -d "ClickHouse" ] && git clone --recursive https://github.com/ClickHouse/ClickHouse.git
docker build -t cloudfuse-io/clickhouse-builder .
docker run -v $(pwd):/build --workdir /build --entrypoint ./build.sh cloudfuse-io/clickhouse-builder

popd
