import os
import logging
import base64
import sys
import time
import subprocess
import socket
from contextlib import closing
import dask.dataframe as dd
from dask_sql import Context
from distributed import Client

logging.getLogger().setLevel(logging.INFO)


IS_COLD_START = True
CLIENT = None
CONTEXT = None


def init():
    global CLIENT, CONTEXT
    subprocess.Popen(
        ["dask-scheduler"], stdout=sys.stdout, stderr=sys.stderr, bufsize=1
    )
    while True:
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as sock:
            if sock.connect_ex(("localhost", 8786)) == 0:
                break

    subprocess.Popen(
        ["dask-worker", "tcp://localhost:8786", "--no-nanny"],
        stdout=sys.stdout,
        stderr=sys.stderr,
        bufsize=1,
    )
    # Client is somehow implicitely used by Dask
    CLIENT = Client("localhost:8786")
    CONTEXT = Context()


def query(sql):
    """Splits the sql statements and return the result of the last one"""
    resp = "Empty response"
    for sql in sql.split(";"):
        stripped_sql = sql.strip()
        if stripped_sql != "":
            plan = CONTEXT.sql(sql.strip())
            # CREATE TABLE statements return None as plan
            if plan is not None:
                resp = str(CONTEXT.sql(sql.strip()).compute())
    return resp


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    global IS_COLD_START

    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    start = time.time()

    if is_cold_start:
        init()
    init_duration = time.time() - start

    # input parameters
    logging.debug("event: %s", event)
    src_command = base64.b64decode(event["cmd"]).decode("utf-8")
    logging.info("command: %s", src_command)
    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v

    logging.debug(CLIENT.scheduler_info())
    resp = query(src_command)
    logging.debug(CLIENT.scheduler_info())

    result = {
        "stdout": resp,
        "stderr": "",
        "env": event.get("env", {}),
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
            "init_duration_sec": init_duration,
        },
    }
    return result


if __name__ == "__main__":
    query_str = f"""
CREATE TABLE nyctaxi WITH (
    location = "s3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/*",
    format = "parquet"
);

SELECT payment_type, SUM(trip_distance) 
FROM nyctaxi
GROUP BY payment_type
"""
    res = handler(
        {"cmd": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
