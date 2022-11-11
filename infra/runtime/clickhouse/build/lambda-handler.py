import base64
import os
import subprocess
import logging
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
        if duration >= 20:
            raise Exception(f"{process_name} timed out after {c} connection attempts")
        time.sleep(0.05)


def init():
    subprocess.Popen(
        ["clickhouse-server", "--config-file=/etc/clickhouse-server/config.xml"],
        stdout=sys.stdout,
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info("server starting...")
    wait_for_socket("server", 9000)


def query(sql: str) -> str:
    subprocess.run(["clickhouse-client", "-q", sql])


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")
    init_duration = time.time() - start

    cli_proc = subprocess.run(
        ["clickhouse-client", "-q", src_command],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    result = {
        "resp": cli_proc.stdout.decode(),
        "logs": cli_proc.stderr.decode(),
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
SELECT payment_type, SUM(trip_distance) 
FROM s3('https://{os.getenv("DATA_BUCKET_NAME")}.s3.{os.getenv("AWS_REGION")}.amazonaws.com//nyc-taxi/2019/01/data.parquet', 'Parquet')
GROUP BY payment_type"""
    res = handler(
        {"query": base64.b64encode(ballista_cmd.encode("utf-8"))},
        {},
    )
    print(res)
