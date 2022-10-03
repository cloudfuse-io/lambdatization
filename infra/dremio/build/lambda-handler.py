from dataclasses import dataclass
import os
import subprocess
import logging
import base64
import sys
import shutil
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
    # os.environ["AWS_CREDENTIAL_PROFILES_FILE"] = "/tmp/aws/credentials"


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
            time.sleep(0.1)
            return create_firstuser(timeout, start_time)
        return Resp(False, "Failed to create first user: time out")


def init() -> Resp:
    """Try to init Dremio, if success return token as msg"""
    if not os.path.exists("/tmp/log"):
        os.makedirs("/tmp/log")
    process = subprocess.run(
        ["/opt/dremio/bin/dremio", "start"], capture_output=True, env=os.environ
    )
    if process.returncode != 0:
        return f"`dremio start` exited with code {process.returncode}:\n{process.stdout}{process.stderr}"
    res = create_firstuser(120, time.time())
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


IS_COLD_START = True


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    shutil.rmtree("/tmp", ignore_errors=True)
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    # Init must run in handler because it is otherwise limited to 10S
    # INIT_RESP can be moved to global to keep in memory accross lambda runs
    INIT_RESP = None
    if is_cold_start:
        setup_credentials()
        INIT_RESP = init()
    if not INIT_RESP.success:
        raise Exception(f"Init failed: {INIT_RESP.msg}")
    init_duration = time.time() - start

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
    except Exception as e:
        query_err = repr(e)

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


# handler(
#     {
#         "cmd": base64.b64encode(
#             f"""SELECT payment_type, SUM(trip_distance) FROM {SOURCE_NAME}."l12n-615900053518-eu-west-1-default"."nyc-taxi"."2019"."01" GROUP BY payment_type""".encode(
#                 "utf-8"
#             )
#         )
#     },
#     {},
# )
