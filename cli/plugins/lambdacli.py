"""Deployment of the L12N CLI image in Lambda"""

import base64
import io
import json
import logging
import os
import random
import subprocess

import awslambdaric.bootstrap
import dotenv
from common import aws, format_lambda_output, terraform_output
from invoke import Exit, task

logging.getLogger().setLevel(logging.INFO)

READ_ONLY_REPO_DIR = "/repo"


@task
def run_bootstrap(c):
    """Call this as the lambda entrypoint"""
    awslambdaric.bootstrap.run(
        f"{READ_ONLY_REPO_DIR}/cli",
        "plugins.lambdacli.handler",
        os.getenv("AWS_LAMBDA_RUNTIME_API"),
    )


@task(autoprint=True)
def invoke(c, command, sampling=None, json_output=False):
    """Invoke the AWS Lambda function with the CLI image

    Commands that need to connect to a Docker server will fail. Local Terraform
    states are not added to the image, so use a remote backend to enable
    commands that use Terraform outputs."""
    lambda_name = terraform_output(c, "lambdacli", "lambda_name")
    cmd_b64 = base64.b64encode(command.encode()).decode()
    body = {"cmd": cmd_b64}
    if sampling is not None:
        body["sampling"] = sampling
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps(body).encode(),
        InvocationType="RequestResponse",
    )
    resp_payload = lambda_res["Payload"].read().decode()
    if "FunctionError" in lambda_res:
        raise Exit(message=resp_payload, code=1)
    return format_lambda_output(resp_payload, json_output)


def handler(event, context):
    """Handler for the AWS Lambda function running the CLI image

    Fields in event object:
    - cmd: base64 encoded command to run
    - sampling: 0.25 means 3 out of 4 runs will be randomly canceled"""

    # Some gymnastic is required to have the repo in the writable location /tmp
    os.system("rm -rf /tmp/*")
    os.system(f"cp -r {READ_ONLY_REPO_DIR} /tmp")
    os.environ["REPO_DIR"] = f"/tmp{READ_ONLY_REPO_DIR}"
    os.environ["PATH"] = f"{os.environ['PATH']}:/tmp{READ_ONLY_REPO_DIR}"

    # Load envfile from secrets
    envfile_str: str = aws("secretsmanager").get_secret_value(
        SecretId=os.environ["ENV_FILE_SECRET_ID"],
        VersionId=os.environ["ENV_FILE_SECRET_VERSION_ID"],
    )["SecretString"]
    dotenv.load_dotenv(stream=io.StringIO(envfile_str), override=True)

    cmd = base64.b64decode(event["cmd"]).decode("utf-8")
    if random.uniform(0, 1) > float(event.get("sampling", 1)):
        logging.info(f"Skipping run of CMD: {cmd}")
        return {"stdout": "Run skipped"}
    res = subprocess.Popen(
        ["/bin/bash", "-c", cmd], stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    logging.info("""=== CMD ===""")
    logging.info(cmd)
    stdout, stderr = res.communicate()
    logging.info("""=== STDOUT ===""")
    logging.info(stdout.decode())
    logging.info("""=== STDERR ===""")
    logging.info(stderr.decode())
    logging.info("""=== RETURNCODE ===""")
    logging.info(res.returncode)
    if res.returncode != 0:
        raise Exception(stderr.decode())
    return {
        "stdout": stdout.decode(),
        "stderr": stderr.decode(),
    }
