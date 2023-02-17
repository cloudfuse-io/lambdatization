import logging
import os
import stat
import subprocess
import sys
import time

import boto3

logging.getLogger().setLevel(logging.INFO)


IS_COLD_START = True


s3 = boto3.client("s3")


def setup_binaries(bucket_name: str, bin_object_key: str, lib_object_key="") -> str:
    if lib_object_key != "":
        local_lib_location = f"/tmp/{lib_object_key}"
        os.makedirs(os.path.dirname(local_lib_location), exist_ok=True)
        s3.download_file(bucket_name, lib_object_key, local_lib_location)
        os.environ["LD_PRELOAD"] = local_lib_location

    local_binary_location = f"/tmp/{bin_object_key}"
    os.makedirs(os.path.dirname(local_binary_location), exist_ok=True)
    s3.download_file(bucket_name, bin_object_key, local_binary_location)
    os.chmod(
        local_binary_location, os.stat(local_binary_location).st_mode | stat.S_IEXEC
    )
    return local_binary_location


def handler(event: dict, context):
    """AWS Lambda handler"""
    start = time.time()
    global IS_COLD_START
    if not IS_COLD_START:
        return {"error": "Not a cold start"}
    IS_COLD_START = False

    bucket_name = event["bucket_name"]
    bin_object_key = event["bin_object_key"]
    lib_object_key = event.get("lib_object_key", "")
    timeout_sec = event.get("timeout_sec", None)
    for name, value in event.get("env", {}).items():
        os.environ[name] = str(value)

    local_binary_location = setup_binaries(bucket_name, bin_object_key, lib_object_key)

    try:
        res = subprocess.run(
            [local_binary_location],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_sec,
        )
        stdout = res.stdout
        stderr = res.stderr
        returncode = res.returncode
    except subprocess.TimeoutExpired as res:
        logging.info(f"{local_binary_location} stopped after {timeout_sec}s timeout")
        stdout = res.stdout
        stderr = res.stderr
        returncode = -1

    result = {
        "stdout": stdout,
        "stderr": stderr,
        "returncode": returncode,
        "context": {
            "handler_duration_sec": time.time() - start,
        },
    }
    return result


if __name__ == "__main__":
    local_binary_location = setup_binaries(sys.argv[1], sys.argv[2])
    os.execl(local_binary_location, local_binary_location)
