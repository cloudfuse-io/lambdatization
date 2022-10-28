import os
import logging
import base64
import sys
import time
import subprocess
import socket
from contextlib import closing
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


def query(sql: str) -> str:
    """Splits the sql statements and return the result of the last one"""
    plan = CONTEXT.sql(sql)
    # CREATE TABLE statements return None as plan
    if plan is not None:
        return str(CONTEXT.sql(sql).compute())
    else:
        return "No plan to compute"


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")

    logging.debug(CLIENT.scheduler_info())
    resp = ""
    parsed_queries = []
    for sql in src_command.split(";"):
        sql = sql.strip()
        if sql == "":
            continue
        parsed_queries.append(sql)
        resp = query(sql)
    logging.debug(CLIENT.scheduler_info())

    result = {
        "resp": resp,
        "logs": "",
        "parsed_queries": parsed_queries,
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
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
        {"query": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
