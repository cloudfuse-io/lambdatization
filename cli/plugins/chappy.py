"""Run Chappy in the cloud"""

import base64
import json
import time
import uuid
from concurrent.futures import ThreadPoolExecutor

import core
from common import (
    AWS_REGION,
    DOCKERDIR,
    REPOROOT,
    FargateService,
    aws,
    terraform_output,
    wait_deployment,
)
from invoke import Context, Exit, task


## FARGATE COMPONENTS ##


def service_outputs(c: Context) -> tuple[str, str, str]:
    cluster = terraform_output(c, "chappy", "fargate_cluster_name")
    family = terraform_output(c, "chappy", "seed_task_family")
    service_name = terraform_output(c, "chappy", "seed_service_name")
    return (cluster, service_name, family)


@task
def seed_status(c):
    """Get the status of the Seed server"""
    print(FargateService(*service_outputs(c)).service_status())


@task
def seed_start(c):
    """Start the Seed server instance as an AWS Fargate task. Noop if a Seed server is already running"""
    FargateService(*service_outputs(c)).start_service()


@task
def seed_stop(c):
    """Stop the Seed server instance"""
    FargateService(*service_outputs(c)).stop_service()


@task(autoprint=True)
def seed_ip(c):
    """Get the public IP of the running seed"""
    for detail in FargateService(*service_outputs(c)).describe_task()["attachments"][0][
        "details"
    ]:
        if detail["name"] == "networkInterfaceId":
            ni_desc = aws("ec2").describe_network_interfaces(
                NetworkInterfaceIds=[detail["value"]]
            )
            return ni_desc["NetworkInterfaces"][0]["Association"]["PublicIp"]
    raise Exit(message="Network interface not found")


@task
def seed_execute(c, cmd="/bin/bash"):
    """Run ad-hoc or interactive commands on the Seed"""
    task_id = FargateService(*service_outputs(c)).get_task_id()
    cluster = terraform_output(c, "chappy", "fargate_cluster_name")
    # if not running the default interactive shell, encode the command to avoid escaping issues
    if cmd != "/bin/bash":
        cmd_bytes = cmd.encode()
        cmd = f"/bin/bash -c 'echo {base64.b64encode(cmd_bytes).decode()} | base64 -d | /bin/bash'"
    # we use the CLI here because boto does not know how to use the session-manager-plugin
    c.run(
        f"""aws ecs execute-command \
		--cluster {cluster} \
		--task {task_id} \
		--interactive \
		--command "{cmd}" \
        --region {AWS_REGION()}""",
        pty=True,
    )


## LAMBDA COMPONENTS ##


def redeploy(lambda_name) -> str:
    wait_deployment(lambda_name)
    aws("lambda").update_function_configuration(
        FunctionName=lambda_name, Description=str(uuid.uuid4())
    )
    wait_deployment(lambda_name)
    response = aws("lambda").publish_version(
        FunctionName=lambda_name,
    )
    return response["Version"]


def invoke_lambda(lambda_name, bucket_name, lambda_version, timeout, bin, lib, env):
    start_time = time.time()
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps(
            {
                "bucket_name": bucket_name,
                "bin_object_key": bin,
                "lib_object_key": lib,
                "timeout_sec": timeout,
                "env": env,
            }
        ).encode(),
        InvocationType="RequestResponse",
        LogType="Tail",
        Qualifier=lambda_version,
    )
    result = []
    result.append(f"==============================")
    result.append(f"RESULTS FOR {bin}")
    result.append(f"EXTERNAL_DURATION: {time.time() - start_time}")
    resp_payload = lambda_res["Payload"].read().decode()
    result.append("== LOGS ==")
    result.append(base64.b64decode(lambda_res["LogResult"]).decode())
    if "FunctionError" in lambda_res:
        raise Exit(message=resp_payload, code=1)
    result.append("== PAYLOAD ==")
    result.append(resp_payload)
    result.append(f"==============================")
    return "\n".join(result)


@task
def run_dev(c, seed=None):
    """Run the Chappy binaries on Lambda using the provided seed public IP"""
    bucket_name = core.bucket_name(c)
    lambda_name = terraform_output(c, "chappy", "dev_lambda_name")
    if seed is None:
        seed = seed_ip(c)

    with ThreadPoolExecutor() as executor:
        redeploy_fut = executor.submit(redeploy, lambda_name)
        c.run(
            f"docker build \
                -f {DOCKERDIR}/chappy/build.Dockerfile \
                -t cloudfuse-io/l12n:chappy-build \
                {REPOROOT}/chappy"
        )
        upload = lambda src, dst: c.run(
            f"docker run --rm --entrypoint cat cloudfuse-io/l12n:chappy-build /target/{src} | \
                aws s3 cp - s3://{bucket_name}/{dst} --region {AWS_REGION()}"
        )
        # lib_up_fut = executor.submit(upload, "debug/libchappy.so", "dev/libchappy.so")
        # client_up_fut = executor.submit(upload, "debug/client", "dev/client")
        server_up_fut = executor.submit(upload, "debug/server", "dev/server")
        lambda_version = redeploy_fut.result()
        # lib_up_fut.result()
        # client_up_fut.result()
        server_up_fut.result()
        server_fut = executor.submit(
            invoke_lambda,
            lambda_name,
            bucket_name,
            lambda_version,
            3,
            "dev/server",
            "",
            {
                "SEED_HOSTNAME": seed,
                "SEED_PORT": 8080,
                "VIRTUAL_SUBNET": "172.28.0.0/16",
                "SERVER_VIRTUAL_IP": "172.28.0.1",
                "CLIENT_VIRTUAL_IP": "172.28.0.2",
                "RUST_LOG": "debug,h2=error",
                "RUST_BACKTRACE": "1",
            },
        )
        print(server_fut.result())
