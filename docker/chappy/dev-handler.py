import logging
import os
import stat
import subprocess
import sys
import time
from io import BytesIO

import boto3

logging.getLogger().setLevel(logging.INFO)


IS_COLD_START = True


s3 = boto3.client("s3")


def setup_binaries(
    bucket_name: str,
    app_object_key: str,
    libchappy_object_key="",
    perforator_object_key="",
) -> tuple[str, str]:
    if libchappy_object_key != "":
        local_lib_location = f"/tmp/{libchappy_object_key}"
        os.makedirs(os.path.dirname(local_lib_location), exist_ok=True)
        s3.download_file(bucket_name, libchappy_object_key, local_lib_location)
        os.environ["LD_PRELOAD"] = local_lib_location

    local_perforator_location = ""
    if perforator_object_key != "":
        local_perforator_location = f"/tmp/{perforator_object_key}"
        os.makedirs(os.path.dirname(local_perforator_location), exist_ok=True)
        s3.download_file(bucket_name, perforator_object_key, local_perforator_location)
        os.chmod(
            local_perforator_location,
            os.stat(local_perforator_location).st_mode | stat.S_IEXEC,
        )

    local_app_location = f"/tmp/{app_object_key}"
    os.makedirs(os.path.dirname(local_app_location), exist_ok=True)
    s3.download_file(bucket_name, app_object_key, local_app_location)
    os.chmod(local_app_location, os.stat(local_app_location).st_mode | stat.S_IEXEC)

    return (local_app_location, local_perforator_location)


def run_perforator(local_perforator_location: str):
    if local_perforator_location == "":
        return (BytesIO(b""), BytesIO(b""))
    perf_proc = subprocess.Popen(
        [local_perforator_location],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return (perf_proc.stdout, perf_proc.stderr)


class Perforator:
    def __init__(self, local_perforator_location: str):
        if local_perforator_location == "":
            self.proc = None
        else:
            self.proc = subprocess.Popen(
                [local_perforator_location],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            time.sleep(0.01)

    def stdout(self):
        if not self.proc is None:
            self.proc.terminate()
            return self.proc.stdout.read().decode()

    def stderr(self):
        if not self.proc is None:
            self.proc.terminate()
            return self.proc.stderr.read().decode()


def handler(event: dict, context):
    """AWS Lambda handler"""
    handler_start = time.time()
    global IS_COLD_START
    if not IS_COLD_START:
        return {"error": "Not a cold start"}
    IS_COLD_START = False

    bucket_name = event["bucket_name"]
    app_object_key = event["app_object_key"]
    libchappy_object_key = event.get("libchappy_object_key", "")
    perforator_object_key = event.get("perforator_object_key", "")
    timeout_sec = event.get("timeout_sec", None)
    for name, value in event.get("env", {}).items():
        os.environ[name] = str(value)

    local_app_location, local_perforator_location = setup_binaries(
        bucket_name, app_object_key, libchappy_object_key, perforator_object_key
    )

    subproc_start = time.time()
    perforator = Perforator(local_perforator_location)
    try:
        res = subprocess.run(
            [local_app_location],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_sec,
        )
        stdout = res.stdout
        stderr = res.stderr
        returncode = res.returncode
    except subprocess.TimeoutExpired as res:
        logging.info(f"{local_app_location} stopped after {timeout_sec}s timeout")
        stdout = res.stdout
        stderr = res.stderr
        returncode = -1

    subproc_duration = time.time() - subproc_start

    result = {
        "stdout": stdout,
        "stderr": stderr,
        "perf_stdout": perforator.stdout(),
        "perf_stderr": perforator.stderr(),
        "returncode": returncode,
        "context": {
            "handler_duration_sec": time.time() - handler_start,
            "subproc_duration_sec": subproc_duration,
        },
    }
    return {k: v for k, v in result.items() if v is not None}


if __name__ == "__main__":
    # exec the app binary only
    (local_app_location, _) = setup_binaries(sys.argv[1], sys.argv[2])
    os.execl(local_app_location, local_app_location)
