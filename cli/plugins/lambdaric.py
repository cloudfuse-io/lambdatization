import base64
import json
import logging
import os
import subprocess

import awslambdaric.bootstrap
from common import aws, terraform_output
from invoke import Exit, task

logging.getLogger().setLevel(logging.INFO)

READ_ONLY_REPO_DIR = "/repo"


@task
def run_bootstrap(c):
    """Call this as the lambda entrypoint"""
    awslambdaric.bootstrap.run(
        f"{READ_ONLY_REPO_DIR}/cli",
        "plugins.lambdaric.handler",
        os.getenv("AWS_LAMBDA_RUNTIME_API"),
    )


@task(autoprint=True)
def invoke(c, command):
    lambda_name = terraform_output(c, "scheduling", "lambda_name")
    cmd_b64 = base64.b64encode(command.encode()).decode()
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps({"cmd": cmd_b64}).encode(),
        InvocationType="RequestResponse",
    )
    resp_payload = lambda_res["Payload"].read().decode()
    if "FunctionError" in lambda_res:
        raise Exit(message=resp_payload, code=1)
    return resp_payload


def handler(event, context):
    """AWS Lambda handler"""

    # Some gymnastic is required to have everything in the writable location /tmp
    os.system("rm -rf /tmp/*")
    os.system(f"cp -r {READ_ONLY_REPO_DIR} /tmp")
    os.environ["REPO_DIR"] = f"/tmp{READ_ONLY_REPO_DIR}"
    os.environ["PATH"] = f"{os.environ['PATH']}:/tmp{READ_ONLY_REPO_DIR}"

    cmd = base64.b64decode(event["cmd"]).decode("utf-8")
    res = subprocess.Popen(
        ["/bin/bash", "-c", cmd], stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    stdout, stderr = res.communicate()
    logging.info(stdout)
    logging.error(stderr)
    return {
        "stdout": stdout.decode(),
        "stderr": stderr.decode(),
        "returncode": res.returncode,
    }
