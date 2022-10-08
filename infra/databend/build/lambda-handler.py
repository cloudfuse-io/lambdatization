import os
import subprocess
import logging
import base64
import sys
import time
import requests

logging.getLogger().setLevel(logging.INFO)
__stdout__ = sys.stdout


IS_COLD_START = True


def init():
    """Start Databend server"""
    subprocess.Popen(["/bootstrap.sh"], stdout=sys.stdout, stderr=sys.stderr, bufsize=1)


def query(sql, timeout):
    """Try to connect to server until timeout is reached to run the query"""
    # Run query
    start_time = time.time()
    logging.info(f"Running {sql}")
    while True:
        try:
            basic = requests.auth.HTTPBasicAuth("root", "root")
            resp = requests.post(
                "http://localhost:8000/v1/query/",
                headers={"Content-Type": "application/json"},
                auth=basic,
                json={"sql": sql},
            )
            json_resp = resp.json()
            if "error" in json_resp and json_resp["error"] is not None:
                raise Exception(json_resp["error"]["message"])
            resp.raise_for_status()
            return json_resp
        except requests.exceptions.ConnectionError:
            if time.time() - start_time < timeout:
                time.sleep(0.2)
            else:
                raise Exception("Attempt to run SQL query timed out")


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

    resp = None
    for sql in src_command.split(";"):
        if sql.strip() != "":
            resp = query(sql.strip(), 30)

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
CREATE TRANSIENT TABLE IF NOT EXISTS taxi201901
(
    payment_type INT,
    trip_distance FLOAT
);

COPY INTO taxi201901
  FROM 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/'
  credentials=(
    aws_key_id='{os.getenv("AWS_ACCESS_KEY_ID")}' 
    aws_secret_key='{os.getenv("AWS_SECRET_ACCESS_KEY")}' 
    aws_token='{os.getenv("AWS_SESSION_TOKEN")}'
  )
  pattern ='.*'
  file_format = (type = 'PARQUET');

SELECT payment_type, SUM(trip_distance) 
FROM taxi201901
GROUP BY payment_type;
"""
    res = handler(
        {"cmd": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
