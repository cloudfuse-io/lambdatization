from dataclasses import dataclass
import os
import subprocess
import logging
import base64
import sys
import time
from typing import Dict
import requests

logging.getLogger().setLevel(logging.INFO)
__stdout__ = sys.stdout

DREMIO_ORIGIN = "http://localhost:9047"
SOURCE_NAME = "s3source"
USER_NAME = "l12n"
USER_PASSWORD = "l12nrocks"


def setup_credentials():
    if not os.path.exists("/tmp/aws"):
        os.makedirs("/tmp/aws")
    with open("/tmp/aws/credentials", "w") as f:
        f.write("[default]\n")
        f.write(f'aws_access_key_id = {os.environ["AWS_ACCESS_KEY_ID"]}\n')
        f.write(f'aws_secret_access_key = {os.environ["AWS_SECRET_ACCESS_KEY"]}\n')
        if "AWS_SESSION_TOKEN" in os.environ:
            f.write(f'aws_session_token = {os.environ["AWS_SESSION_TOKEN"]}\n')


@dataclass
class Resp:
    success: bool
    msg: str


def create_firstuser(timeout, start_time) -> Resp:
    """Repeatedly call the PUT bootstrap/firstuser API until getting a response"""
    try:
        resp = requests.put(
            f"{DREMIO_ORIGIN}/apiv2/bootstrap/firstuser",
            headers={
                "Authorization": "_dremionull",
                "Content-Type": "application/json",
            },
            json={
                "userName": USER_NAME,
                "firstName": "lamb",
                "lastName": "da",
                "email": "l12n@cloudfuse.io",
                "createdAt": int(time.time() * 1000),
                "password": USER_PASSWORD,
            },
        )
        if resp.status_code != 200:
            return Resp(
                False, f"Failed to create first user ({resp.status_code}):\n{resp.text}"
            )
        return Resp(True, "")
    except requests.exceptions.ConnectionError:
        if time.time() - start_time < timeout:
            time.sleep(0.2)
            return create_firstuser(timeout, start_time)
        return Resp(False, "Failed to create first user: time out")


def init() -> Resp:
    """Try to init Dremio, if success return token as msg"""
    setup_credentials()
    if not os.path.exists("/tmp/log"):
        os.makedirs("/tmp/log")
    process = subprocess.run(["/opt/dremio/bin/dremio", "start"], capture_output=True)
    if process.returncode != 0:
        return f"`dremio start` exited with code {process.returncode}:\n{process.stdout}{process.stderr}"
    res = create_firstuser(240, time.time())
    if not res.success:
        return res
    login_resp = requests.post(
        f"{DREMIO_ORIGIN}/apiv2/login",
        headers={
            "Authorization": "_dremionull",
            "Content-Type": "application/json",
        },
        json={"userName": USER_NAME, "password": USER_PASSWORD},
    )
    if login_resp.status_code != 200:
        return Resp(False, "Failed to login to dremio")

    token = login_resp.json()["token"]

    source_req = {
        "name": SOURCE_NAME,
        "config": {
            "credentialType": "AWS_PROFILE",
            "awsProfile": "default",
            "externalBucketList": [],
            "enableAsync": True,
            "enableFileStatusCheck": True,
            "rootPath": "/",
            "defaultCtasFormat": "ICEBERG",
            "propertyList": [],
            "whitelistedBuckets": [],
            "isCachingEnabled": False,
            "maxCacheSpacePct": 100,
        },
        "accelerationRefreshPeriod": 3600000,
        "accelerationGracePeriod": 10800000,
        "accelerationNeverExpire": False,
        "accelerationNeverRefresh": False,
        "metadataPolicy": {
            "deleteUnavailableDatasets": False,
            "autoPromoteDatasets": True,
            "namesRefreshMillis": 3600000,
            "datasetDefinitionRefreshAfterMillis": 3600000,
            "datasetDefinitionExpireAfterMillis": 10800000,
            "authTTLMillis": 86400000,
            "updateMode": "PREFETCH_QUERIED",
        },
        "type": "S3",
        "accessControlList": {"userControls": [], "roleControls": []},
    }

    resp = requests.put(
        f"{DREMIO_ORIGIN}/apiv2/source/{SOURCE_NAME}/",
        headers={
            "Authorization": f"_dremio{token}",
            "Content-Type": "application/json",
        },
        json=source_req,
    )
    if resp.status_code != 200:
        return Resp(
            False, f"Failed to create Dremio source ({resp.status_code}):\n{resp.text}"
        )

    return Resp(True, token)


def query(query: str, token: str) -> Dict:
    """Run the provided SQL query on the currently inited profile"""
    sql_req = {
        "sql": query,
        "context": [f"@{USER_NAME}"],
        "references": {},
    }

    resp = requests.post(
        f"{DREMIO_ORIGIN}/apiv2/datasets/new_untitled_sql_and_run?newVersion=1",
        headers={
            "Authorization": f"_dremio{token}",
            "Content-Type": "application/json",
        },
        json=sql_req,
    )

    logging.debug(resp.text)
    resp.raise_for_status()

    job_id = resp.json()["jobId"]["id"]
    logging.info(f"job_id: {job_id}")

    while True:
        job_resp = requests.get(
            f"http://localhost:9047/api/v3/job/{job_id}",
            headers={
                "Authorization": f"_dremio{token}",
                "Content-Type": "application/json",
            },
        )
        logging.debug(job_resp.text)
        job_resp.raise_for_status()
        job_resp_json = job_resp.json()
        if job_resp_json["jobState"] in ["COMPLETED", "CANCELED", "FAILED"]:
            job_resp = requests.get(
                f"http://localhost:9047/api/v3/job/{job_id}/results",
                headers={
                    "Authorization": f"_dremio{token}",
                    "Content-Type": "application/json",
                },
            )
            logging.debug(job_resp.text)
            job_resp.raise_for_status()
            return job_resp.json()


# def clean():
#     # subprocess.run(["tail", "/tmp/log/server.log"], capture_output=True)
#     try:
#         a_file = open("/tmp/log/server.log")
#         file_contents = a_file.read()
#         print("===============================================")
#         print(file_contents)
#         print("===============================================")
#     except:
#         print("couldn't read /tmp/log/server.log")
#     subprocess.run(["/opt/dremio/bin/dremio", "stop"], capture_output=True)
#     shutil.rmtree("/tmp", ignore_errors=True)


IS_COLD_START = True


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    global IS_COLD_START, INIT_RESP

    logging.warning("Dremio source init fails if user is not dremio or sbx_user1051")

    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    start = time.time()
    # Init must run in handler because it is otherwise limited to 10S
    if is_cold_start:
        INIT_RESP = init()
    init_duration = time.time() - start

    if not INIT_RESP.success:
        raise Exception(f"Init failed: {INIT_RESP.msg}")

    # input parameters
    logging.debug("event: %s", event)
    src_command = base64.b64decode(event["cmd"]).decode("utf-8")
    logging.info("command: %s", src_command)
    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v

    query_res = ""
    query_err = ""
    try:
        query_res = query(src_command, INIT_RESP.msg)
    except requests.exceptions.HTTPError as err:
        query_err = f"Error calling {err.request.url} : {err}\n{err.response.text}"

    result = {
        "stdout": query_res,
        "stderr": query_err,
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
SELECT payment_type, SUM(trip_distance) 
FROM {SOURCE_NAME}."{os.getenv("DATA_BUCKET_NAME")}"."nyc-taxi"."2019"."01" 
GROUP BY payment_type
"""
    res = handler(
        {"cmd": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
