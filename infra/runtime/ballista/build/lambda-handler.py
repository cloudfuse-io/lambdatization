import base64
import os
from typing import List
import subprocess
import logging
from pexpect import popen_spawn
import time
import sys
import socket
from contextlib import closing


logging.getLogger().setLevel(logging.INFO)

process_cli = None
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


def start_cli():
    global process_cli
    logging.info("cli starts")
    process_cli = popen_spawn.PopenSpawn(
        [
            "/opt/ballista/ballista-cli",
            "--host",
            "localhost",
            "--port",
            "50050",
            "--format",
            "csv",
        ]
    )
    process_cli.expect(b"Ballista CLI v[0-9.]*")
    logging.debug(process_cli.before)


def init():
    sched_cmd = [
        "/opt/ballista/ballista-scheduler",
        "--sled-dir",
        "/tmp/scheduler/sled",
    ]
    start_server("scheduler", sched_cmd, 50050)
    start_server("executor", ["/opt/ballista/ballista-executor"], 50051)
    start_cli()


def query(sql: str) -> str:
    logging.info(f"Running {sql}")
    process_cli.sendline(sql)
    look_for = [
        b"Query took ([0-9]*[.])?[0-9]+ seconds.\\\n",
        b"DataFusionError\(",
        b"Invalid statement",
    ]
    i = process_cli.expect(look_for, timeout=200)

    cli_output = process_cli.before.decode("utf-8")
    if i == 0:
        logging.info(f"Result: {cli_output}")
        return cli_output
    else:
        process_cli.expect([b"\n"], timeout=1)
        cli_err = f"{cli_output}{look_for[i]}{process_cli.before.decode()}"
        raise Exception(f"Query failed: {cli_err}")


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

    resp = ""
    parsed_queries = []
    for sql in src_command.split(";"):
        sql = sql.strip() + ";"
        if sql == ";":
            continue
        parsed_queries.append(sql)
        resp = query(sql)

    result = {
        "resp": resp,
        "logs": "",
        "parsed_queries": parsed_queries,
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
