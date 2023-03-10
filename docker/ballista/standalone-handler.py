import base64
import logging
import os
import socket
import subprocess
import sys
import time
from contextlib import closing
from typing import List

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True


def wait_for_socket(process_name: str, port: int):
    c = 0
    start_time = time.time()
    while True:
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as sock:
            s = sock.connect_ex(("localhost", port))
            duration = time.time() - start_time
            c += 1
            if s == 0:
                msg = f"{process_name} up after {duration} secs and {c} connection attempts"
                logging.info(msg)
                break
        if duration >= 5:
            raise Exception(f"{process_name} timed out after {c} connection attempts")
        time.sleep(0.02)


def start_server(name: str, cmd: List[str], port: int):
    subprocess.Popen(
        cmd,
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info(f"{name} starting...")
    wait_for_socket(name, port)


def run_cli(sql: str) -> tuple[str, str]:
    logging.info("cli starts")
    with open("/tmp/sql_query.tmp", "w") as tmp_sql:
        tmp_sql.write(sql)
    process_cli = subprocess.Popen(
        [
            "/opt/ballista/ballista-cli",
            "--host",
            "localhost",
            "--port",
            "50050",
            "--format",
            "csv",
            "--file",
            "/tmp/sql_query.tmp",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    stdout, stderr = process_cli.communicate(input=sql.encode())
    return stdout.decode(), stderr.decode()


def init():
    sched_cmd = [
        "/opt/ballista/ballista-scheduler",
        "--sled-dir",
        "/tmp/scheduler/sled",
    ]
    start_server("scheduler", sched_cmd, 50050)
    start_server("executor", ["/opt/ballista/ballista-executor"], 50051)


def handler(event, context):
    """AWS Lambda handler"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")
    init_duration = time.time() - start

    resp, logs = run_cli(src_command)

    result = {
        "resp": resp,
        "logs": logs,
        "parsed_queries": [src_command],
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
            "init_duration_sec": init_duration,
        },
    }
    return result


if __name__ == "__main__":
    ballista_cmd = f"""
CREATE EXTERNAL TABLE trips01 STORED AS PARQUET
 LOCATION 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) FROM trips01
 GROUP BY payment_type;"""
    res = handler(
        {"query": base64.b64encode(ballista_cmd.encode("utf-8"))},
        {},
    )
    print(res)
