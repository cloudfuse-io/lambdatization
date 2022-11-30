import os
import subprocess
import logging
import base64
import sys
import time
import requests

logging.getLogger().setLevel(logging.INFO)


IS_COLD_START = True
SESSION_CREDENTIALS = f"""
    aws_key_id='{os.getenv("AWS_ACCESS_KEY_ID")}' 
    aws_secret_key='{os.getenv("AWS_SECRET_ACCESS_KEY")}' 
    aws_token='{os.getenv("AWS_SESSION_TOKEN")}'
  """


def init():
    """Start Databend server"""
    subprocess.Popen(["/bootstrap.sh"], stdout=sys.stdout, stderr=sys.stderr, bufsize=1)


def query(sql, timeout):
    """Try to connect to server until timeout is reached to run the query"""
    # Run query
    start_time = time.time()
    logging.info(f"Running {sql}")
    sql = sql.replace("__RUNTIME_PROVIDED__", SESSION_CREDENTIALS)
    while True:
        try:
            basic = requests.auth.HTTPBasicAuth("root", "root")
            resp = requests.post(
                "http://localhost:8000/v1/query/",
                headers={"Content-Type": "application/json"},
                auth=basic,
                json={"sql": sql, "pagination": {"wait_time_secs": 1000}},
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
    """AWS Lambda handler"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")

    resp = ""
    parsed_queries = []
    for sql in src_command.split(";"):
        sql = sql.strip()
        if sql == "":
            continue
        resp = query(sql, 30)
        parsed_queries.append(sql)

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
CREATE TRANSIENT TABLE taxi201901
(
    payment_type VARCHAR,
    trip_distance FLOAT
);

COPY INTO taxi201901
  FROM 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/'
  credentials=(__RUNTIME_PROVIDED__)
  pattern ='.*[.]parquet'
  file_format = (type = 'PARQUET');

SELECT payment_type, SUM(trip_distance) 
FROM taxi201901
GROUP BY payment_type;
"""
    res = handler(
        {"query": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
