import logging
import os
import stat
import subprocess
import sys
import tempfile
import time

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


class Perforator:
    def __init__(self, bin_path):
        self.tmp_file = tempfile.NamedTemporaryFile(mode="w+", delete=True)
        self.proc = subprocess.Popen(
            [bin_path],
            stderr=self.tmp_file,
        )
        self.logs = ""

    def _load_logs(self):
        if self.logs == "":
            self.proc.terminate()
            try:
                self.proc.communicate(timeout=5)
                logging.info("Perforator successfully terminated")
            except subprocess.TimeoutExpired:
                logging.error("Perforator could not terminate properly")
                self.proc.kill()
                self.proc.communicate()
            self.tmp_file.seek(0)
            self.logs = self.tmp_file.read().strip()
            self.tmp_file.close()

    def get_logs(self) -> str:
        self._load_logs()
        return self.logs

    def log(self, log=logging.info):
        perf_logs_prefixed = "\n".join(
            [f"[PERFORATOR] {line}" for line in self.get_logs().split("\n")]
        )
        log(f"=> PERFORATOR LOGS:\n{perf_logs_prefixed}")


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
        "perforator_logs": perforator.get_logs(),
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
