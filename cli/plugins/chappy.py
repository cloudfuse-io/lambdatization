"""Run Chappy in the cloud"""

import base64
import json
import time
import uuid
from collections import namedtuple
from concurrent.futures import ThreadPoolExecutor

import core
import dynaconf
from common import (
    AWS_REGION,
    DOCKERDIR,
    REPOROOT,
    FargateService,
    aws,
    conf,
    format_lambda_output,
    terraform_output,
    wait_deployment,
)
from invoke import Context, Exit, task

VALIDATORS = [
    dynaconf.Validator("L12N_CHAPPY_OPENTELEMETRY_APIKEY", ne=""),
]

## FARGATE COMPONENTS ##


def service_outputs(c: Context) -> tuple[str, str, str]:
    with ThreadPoolExecutor() as ex:
        cluster_fut = ex.submit(terraform_output, c, "chappy", "fargate_cluster_name")
        family_fut = ex.submit(terraform_output, c, "chappy", "seed_task_family")
        service_name_fut = ex.submit(terraform_output, c, "chappy", "seed_service_name")
    return (cluster_fut.result(), service_name_fut.result(), family_fut.result())


BuildOutput = namedtuple("BuildOutput", ["build_image", "output_dir"])


def build_chappy(c: Context, release=False) -> BuildOutput:
    target_img = "cloudfuse-io/l12n:chappy-build"
    if release:
        output_dir = "/target/release"
        build_flag = "--release"
    else:
        output_dir = "/target/debug"
        build_flag = ""
    c.run(
        f"docker build \
            --build-arg BUILD_FLAG={build_flag} \
            -f {DOCKERDIR}/chappy/build.Dockerfile \
            -t {target_img} \
            {REPOROOT}/chappy"
    )
    return BuildOutput(build_image=target_img, output_dir=output_dir)


def to_s3_key(output_dir, file) -> str:
    return f"dev{output_dir}/{file}"


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
def seed_exec(c, cmd="/bin/bash", pty=True):
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
        --container server \
		--interactive \
		--command "{cmd}" \
        --region {AWS_REGION()}""",
        pty=pty,
    )


@task
def run_seed(c, release=False):
    bucket_name = core.bucket_name(c)
    build_image, output_dir = build_chappy(c, release)
    file = "chappy-seed"
    s3_key = to_s3_key(output_dir, file)

    c.run(
        f"docker run --rm --entrypoint cat {build_image} {output_dir}/{file} | \
            aws s3 cp - s3://{bucket_name}/{s3_key} --region {AWS_REGION()}"
    )
    seed_exec(c, f"python3 dev-handler.py {bucket_name} {s3_key}", pty=False)


@task
def output(c):
    start = time.time()
    print(terraform_output(c, "chappy", "fargate_cluster_name"))
    print(f"duration:{time.time()-start}")


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


def format_lambda_result(name, external_duration, lambda_res):
    result = []
    result.append(f"==============================")
    result.append(f"RESULTS FOR {name}")
    result.append(f"EXTERNAL_DURATION: {external_duration}")
    result.append("== LOGS ==")
    result.append(base64.b64decode(lambda_res["LogResult"]).decode())
    if "FunctionError" in lambda_res:
        raise Exit(message=lambda_res["Payload"], code=1)
    result.append("== PAYLOAD ==")
    result.append(format_lambda_output(lambda_res["Payload"], False))
    result.append(f"==============================")
    return "\n".join(result)


def invoke_lambda(
    lambda_name,
    bucket_name,
    lambda_version,
    timeout,
    app_bin,
    perf_bin,
    chappy_lib,
    env,
) -> tuple[dict, float]:
    start_time = time.time()
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps(
            {
                "bucket_name": bucket_name,
                "app_object_key": app_bin,
                "perforator_object_key": perf_bin,
                "libchappy_object_key": chappy_lib,
                "timeout_sec": timeout,
                "env": env,
            }
        ).encode(),
        InvocationType="RequestResponse",
        LogType="Tail",
        Qualifier=lambda_version,
    )
    lambda_res["Payload"] = lambda_res["Payload"].read().decode()
    return (lambda_res, time.time() - start_time)


@task
def run_lambda_pair(c, seed=None, release=False, client="example-client"):
    """Run the Chappy binaries on Lambda using the provided seed public IP"""
    bucket_name = core.bucket_name(c)
    lambda_name = terraform_output(c, "chappy", "dev_lambda_name")
    if seed is None:
        seed = seed_ip(c)

    with ThreadPoolExecutor() as executor:
        redeploy_fut = executor.submit(redeploy, lambda_name)
        build_image, output_dir = build_chappy(c, release)
        upload = lambda file: c.run(
            f"docker run --rm --entrypoint cat {build_image} {output_dir}/{file} | \
                aws s3 cp - s3://{bucket_name}/{to_s3_key(output_dir, file)} --region {AWS_REGION()}"
        )
        lib_up_fut = executor.submit(upload, "libchappy.so")
        client_up_fut = executor.submit(upload, client)
        server_up_fut = executor.submit(upload, "example-server")
        perforator_up_fut = executor.submit(upload, "chappy-perforator")
        lambda_version = redeploy_fut.result()
        lib_up_fut.result()
        client_up_fut.result()
        server_up_fut.result()
        perforator_up_fut.result()

        common_env = {
            "CHAPPY_SEED_HOSTNAME": seed,
            "CHAPPY_SEED_PORT": 8000,
            "CHAPPY_VIRTUAL_SUBNET": "172.28.0.0/16",
            "RUST_LOG": "info,chappy_perforator=debug,chappy=debug",
            "RUST_BACKTRACE": "1",
        }

        if "L12N_CHAPPY_OPENTELEMETRY_APIKEY" in conf(VALIDATORS):
            common_env["CHAPPY_OPENTELEMETRY_APIKEY"] = conf(VALIDATORS)[
                "L12N_CHAPPY_OPENTELEMETRY_APIKEY"
            ]

        server_fut = executor.submit(
            invoke_lambda,
            lambda_name,
            bucket_name,
            lambda_version,
            20,
            to_s3_key(output_dir, "example-server"),
            to_s3_key(output_dir, "chappy-perforator"),
            to_s3_key(output_dir, "libchappy.so"),
            {**common_env, "CHAPPY_VIRTUAL_IP": "172.28.0.1"},
        )

        time.sleep(0.5)
        mb_sent = 50

        client_fut = executor.submit(
            invoke_lambda,
            lambda_name,
            bucket_name,
            lambda_version,
            19,
            to_s3_key(output_dir, client),
            to_s3_key(output_dir, "chappy-perforator"),
            to_s3_key(output_dir, "libchappy.so"),
            {
                **common_env,
                "CHAPPY_VIRTUAL_IP": "172.28.0.2",
                "SERVER_VIRTUAL_IP": "172.28.0.1",
                "BATCH_SIZE": 32 * 1024,
                "BYTES_SENT": mb_sent * 1024 * 1024,
            },
        )

        (client_result, client_duration) = client_fut.result()
        print(format_lambda_result("CLIENT", client_duration, client_result))
        (server_result, server_duration) = server_fut.result()
        print(format_lambda_result("SERVER", server_duration, server_result))
        # show bandwidth
        client_payload = json.loads(client_result["Payload"])
        if client_payload.get("returncode", None) != 0:
            print(f'Client failed with code {client_payload.get("returncode", None)}')
        elif (
            "context" in client_payload
            and "subproc_duration_sec" in client_payload["context"]
        ):
            # x8 -> conversion from bytes to bits
            # x2 -> bytes are echoed so transfered twice
            print(
                f'=> BANDWIDTH={mb_sent*8*2/client_payload["context"]["subproc_duration_sec"]} Mbit/s'
            )


@task
def run_lambda_cluster(c, seed=None, release=False, binary="example-n-to-n", nodes=5):
    """Run the Chappy binaries on Lambda using the provided seed public IP"""
    bucket_name = core.bucket_name(c)
    lambda_name = terraform_output(c, "chappy", "dev_lambda_name")
    if seed is None:
        seed = seed_ip(c)

    with ThreadPoolExecutor(max_workers=max(4, nodes)) as executor:
        redeploy_fut = executor.submit(redeploy, lambda_name)
        build_image, output_dir = build_chappy(c, release)
        upload = lambda file: c.run(
            f"docker run --rm --entrypoint cat {build_image} {output_dir}/{file} | \
                aws s3 cp - s3://{bucket_name}/{to_s3_key(output_dir, file)} --region {AWS_REGION()}"
        )
        lib_up_fut = executor.submit(upload, "libchappy.so")
        binary_up_fut = executor.submit(upload, binary)
        perforator_up_fut = executor.submit(upload, "chappy-perforator")

        lambda_version = redeploy_fut.result()
        lib_up_fut.result()
        binary_up_fut.result()
        perforator_up_fut.result()

        common_env = {
            "CHAPPY_SEED_HOSTNAME": seed,
            "CHAPPY_SEED_PORT": 8000,
            "CHAPPY_VIRTUAL_SUBNET": "172.28.0.0/16",
            "CLUSTER_IPS": ",".join([f"172.28.0.{i+1}" for i in range(nodes)]),
            "BATCH_SIZE": 32,
            "BYTES_SENT": 128,
            "RUST_LOG": "info,chappy_perforator=debug,chappy=debug",
            "RUST_BACKTRACE": "1",
        }
        if "L12N_CHAPPY_OPENTELEMETRY_APIKEY" in conf(VALIDATORS):
            common_env["CHAPPY_OPENTELEMETRY_APIKEY"] = conf(VALIDATORS)[
                "L12N_CHAPPY_OPENTELEMETRY_APIKEY"
            ]
        node_futs = []
        for i in range(nodes):
            node_fut = executor.submit(
                invoke_lambda,
                lambda_name,
                bucket_name,
                lambda_version,
                20,
                to_s3_key(output_dir, binary),
                to_s3_key(output_dir, "chappy-perforator"),
                to_s3_key(output_dir, "libchappy.so"),
                {**common_env, "CHAPPY_VIRTUAL_IP": f"172.28.0.{i+1}"},
            )
            node_futs.append(node_fut)
        print("nodes scheduled")
        returncodes = []
        durations = []
        for node_fut in node_futs:
            (node_result, node_duration) = node_fut.result()
            payload = json.loads(node_result.get("Payload", "{}"))
            returncodes.append(payload.get("returncode", None))
            durations.append(node_duration)
            print(format_lambda_result("NODE", node_duration, node_result))
        print(f"returncodes: {returncodes}")
        print(f"external durations: {durations}")
