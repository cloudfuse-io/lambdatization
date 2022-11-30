import os
import subprocess
import logging
import base64
import time
from typing import Dict
import requests

logging.getLogger().setLevel(logging.INFO)

DREMIO_ORIGIN = "http://localhost:9047"
SOURCE_NAME = "s3source"
USER_NAME = "l12n"
USER_PASSWORD = "l12nrocks"
NULL_AUTH = {"Authorization": "_dremionull"}
# This global will be completed once auth is successful
TOKEN_AUTH = {"Authorization": None}
IS_COLD_START = True


def setup_aws_credentials():
    """Write AWS credential env variables into an AWS credentials file"""
    if not os.path.exists("/tmp/aws"):
        os.makedirs("/tmp/aws")
    with open("/tmp/aws/credentials", "w") as f:
        f.write("[default]\n")
        f.write(f'aws_access_key_id = {os.environ["AWS_ACCESS_KEY_ID"]}\n')
        f.write(f'aws_secret_access_key = {os.environ["AWS_SECRET_ACCESS_KEY"]}\n')
        if "AWS_SESSION_TOKEN" in os.environ:
            f.write(f'aws_session_token = {os.environ["AWS_SESSION_TOKEN"]}\n')


def create_firstuser(timeout, start_time):
    """Repeatedly call the PUT bootstrap/firstuser API until getting a response"""
    try:
        resp = requests.put(
            f"{DREMIO_ORIGIN}/apiv2/bootstrap/firstuser",
            headers=NULL_AUTH,
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
            raise Exception(
                f"Failed to create first user ({resp.status_code}):\n{resp.text}"
            )
    except requests.exceptions.ConnectionError:
        if time.time() - start_time < timeout:
            time.sleep(0.2)
            return create_firstuser(timeout, start_time)
        raise Exception("Failed to create first user: time out")


def init():
    """Try to init Dremio, if success return token as msg"""
    setup_aws_credentials()
    if not os.path.exists("/tmp/log"):
        os.makedirs("/tmp/log")
    process = subprocess.run(["/opt/dremio/bin/dremio", "start"], capture_output=True)
    if process.returncode != 0:
        raise Exception(
            f"`dremio start` exited with code {process.returncode}:\n{process.stdout}{process.stderr}"
        )
    create_firstuser(240, time.time())
    login_resp = requests.post(
        f"{DREMIO_ORIGIN}/apiv2/login",
        headers=NULL_AUTH,
        json={"userName": USER_NAME, "password": USER_PASSWORD},
    )
    if login_resp.status_code != 200:
        raise Exception("Failed to login to dremio")

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

    TOKEN_AUTH["Authorization"] = f"_dremio{token}"

    resp = requests.put(
        f"{DREMIO_ORIGIN}/apiv2/source/{SOURCE_NAME}/",
        headers=TOKEN_AUTH,
        json=source_req,
    )
    if resp.status_code != 200:
        raise Exception(
            f"Failed to create Dremio source ({resp.status_code}):\n{resp.text}"
        )


def query(query: str) -> Dict:
    """Run the provided SQL query on the currently inited profile"""
    sql_req = {
        "sql": query,
        "context": [f"@{USER_NAME}"],
        "references": {},
    }

    resp = requests.post(
        f"{DREMIO_ORIGIN}/apiv2/datasets/new_untitled_sql_and_run?newVersion=1",
        headers=TOKEN_AUTH,
        json=sql_req,
    )

    logging.debug(resp.text)
    resp.raise_for_status()

    job_id = resp.json()["jobId"]["id"]
    logging.info(f"job_id: {job_id}")

    while True:
        job_resp = requests.get(
            f"{DREMIO_ORIGIN}/api/v3/job/{job_id}",
            headers=TOKEN_AUTH,
        )
        logging.debug(job_resp.text)
        job_resp.raise_for_status()
        job_resp_json = job_resp.json()
        if job_resp_json["jobState"] in ["COMPLETED", "CANCELED", "FAILED"]:
            job_resp = requests.get(
                f"{DREMIO_ORIGIN}/api/v3/job/{job_id}/results",
                headers=TOKEN_AUTH,
            )
            logging.debug(job_resp.text)
            job_resp.raise_for_status()
            return job_resp.json()


def handler(event, context):
    """AWS Lambda handler"""
    logging.warning("Dremio source init fails if user is not dremio or sbx_user1051")

    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")

    query_res = query(src_command)

    result = {
        "resp": query_res,
        "logs": "",
        "parsed_queries": [src_command],
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
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
        {"query": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
