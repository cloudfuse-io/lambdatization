from dataclasses import dataclass
import os
import subprocess
import logging
import base64
import sys
import io
import json
import shutil
import time
import requests
from threading import Thread

logging.getLogger().setLevel(logging.INFO)
__stdout__ = sys.stdout

DREMIO_ORIGIN = "http://localhost:9047"
SOURCE_NAME = "s3source"
USER_NAME = "l12n"
USER_PASSWORD = "l12nrocks"


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
                False, f"Failed to create first user ({resp.status_code}):\n{resp.txt}"
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
    process = subprocess.run(["/opt/dremio/bin/dremio", "start"], capture_output=True)
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
            "credentialType": "ACCESS_KEY",
            "accessKey": os.environ["AWS_ACCESS_KEY_ID"],
            "accessSecret": os.environ["AWS_SECRET_ACCESS_KEY"],
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


INIT_RESP = None
IS_COLD_START = True


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    # Init must run in handler because it is otherwise limited to 10S
    if is_cold_start:
        INIT_RESP = init()
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

    sql_req = {
        # TODO: make bucket name dynamic somehow
        "sql": f'SELECT * FROM {SOURCE_NAME}."l12n-615900053518-eu-west-1-default"."nyc-taxi"."2019"."01"  LIMIT 10',
        "context": [f"@{USER_NAME}"],
        "references": {},
    }

    resp = requests.post(
        f"{DREMIO_ORIGIN}/apiv2/datasets/new_untitled_sql_and_run?newVersion=1",
        headers={
            "Authorization": f"_dremio{INIT_RESP.msg}",
            "Content-Type": "application/json",
        },
        json=sql_req,
    )

    print(resp.text)
    resp.raise_for_status()

    shutil.rmtree("/tmp", ignore_errors=True)
    result = {
        "stdout": "",
        "stderr": "",
        "parsed_cmd": "",
        "returncode": "",
        "env": event.get("env", {}),
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
        },
    }
    return result


handler({"cmd": base64.b64encode("SELECT 1;".encode("utf-8"))}, {})
