import logging
import base64
import os
import tempfile
import time
import sys
import subprocess
from typing import Tuple

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True


def init():
    trino_proc = subprocess.Popen(
        ["launcher", "run"],
        stdout=sys.stdout,
        stderr=subprocess.PIPE,
    )

    subprocess.check_output(
        ["schematool", "-initSchema", "-dbType", "derby"], stderr=sys.stderr, cwd="/tmp"
    )
    subprocess.Popen(
        ["start-metastore"], stdout=sys.stdout, stderr=sys.stderr, cwd="/tmp"
    )

    for line_bytes in trino_proc.stderr:
        log_line = line_bytes.decode()
        print(log_line, flush=True, file=sys.stderr, end="")
        if "======== SERVER STARTED ========" in log_line:
            return
    raise Exception("Trino server didn't start successfully")


def query(sql: str) -> Tuple[str, str]:
    """Run a single SQL query using Trino cli"""
    with tempfile.NamedTemporaryFile(prefix="query", delete=False) as tmp:
        query_file = tmp.name
        tmp.write(sql.encode())

    cli_proc = subprocess.run(
        ["trino", f"--file={query_file}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if cli_proc.returncode != 0:
        raise Exception(f"Query failed: {cli_proc.stderr.decode()}")
    else:
        return (cli_proc.stdout.decode(), cli_proc.stderr.decode())


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")

    (resp_stdout, resp_stderr) = query(src_command)

    result = {
        "resp": resp_stdout,
        "logs": resp_stderr,
        "parsed_queries": [src_command],
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
        },
    }
    return result


if __name__ == "__main__":
    query_str = f"""
CREATE TABLE hive.default.taxi201901 (trip_distance REAL, payment_type VARCHAR)
WITH (
  external_location = 's3a://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/',
  format = 'PARQUET'
);

SELECT payment_type, SUM(trip_distance)
FROM hive.default.taxi201901
GROUP BY payment_type;
"""
    res = handler(
        {"query": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
