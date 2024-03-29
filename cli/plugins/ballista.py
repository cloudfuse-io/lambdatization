"""Ballista on AWS Lambda"""

import base64
import json
import random
import string
import time
from concurrent.futures import ThreadPoolExecutor

import core
from common import (
    OTEL_VALIDATORS,
    aws,
    conf,
    format_lambda_output,
    get_otel_env,
    terraform_output,
)
from invoke import Exit, task

VALIDATORS = OTEL_VALIDATORS


def rand_cluster_id() -> str:
    return "".join(
        random.choice(string.digits + string.ascii_letters) for _ in range(6)
    )


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """CREATE EXTERNAL TABLE and find out SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
CREATE EXTERNAL TABLE nyctaxi2019{month} STORED AS PARQUET
LOCATION 's3://{core.bucket_name(c)}/nyc-taxi/2019/{month}/';
SELECT payment_type, SUM(trip_distance) FROM nyctaxi2019{month}
GROUP BY payment_type;"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "ballista", sql, json_output=json_output)


def format_lambda_result(name, external_duration, lambda_res):
    result = []
    result.append(f"==============================")
    result.append(f"RESULTS FOR {name}")
    result.append(f"EXTERNAL_DURATION: {external_duration}")
    if "FunctionError" in lambda_res:
        raise Exit(message=lambda_res["Payload"], code=1)
    result.append("== PAYLOAD ==")
    result.append(format_lambda_output(lambda_res["Payload"], False))
    result.append(f"==============================")
    return "\n".join(result)


def run_executor(
    lambda_name: str,
    bucket_name: str,
    seed_ip: str,
    virtual_ip: str,
    scheduler_ip: str,
    nodes: int,
    cluster_id: str,
):
    start_time = time.time()
    env = {
        "CHAPPY_CLUSTER_SIZE": nodes,
        "CHAPPY_SEED_HOSTNAME": seed_ip,
        "CHAPPY_SEED_PORT": 8000,
        "CHAPPY_VIRTUAL_IP": virtual_ip,
        "RUST_LOG": "info,chappy_perforator=debug,chappy=debug,rustls=error",
        "CHAPPY_CLUSTER_ID": cluster_id,
        "RUST_BACKTRACE": "1",
        **get_otel_env(),
    }
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps(
            {
                "role": "executor",
                "bucket_name": bucket_name,
                "timeout_sec": 40,
                "scheduler_ip": scheduler_ip,
                "env": env,
            }
        ).encode(),
        InvocationType="RequestResponse",
        LogType="None",
    )
    lambda_res["Payload"] = lambda_res["Payload"].read().decode()
    return (lambda_res, time.time() - start_time)


def run_scheduler(
    lambda_name: str,
    bucket_name: str,
    seed_ip: str,
    virtual_ip: str,
    query: str,
    nodes: int,
    cluster_id: str,
):
    start_time = time.time()
    env = {
        "CHAPPY_CLUSTER_SIZE": nodes,
        "CHAPPY_SEED_HOSTNAME": seed_ip,
        "CHAPPY_SEED_PORT": 8000,
        "CHAPPY_CLUSTER_ID": cluster_id,
        "CHAPPY_VIRTUAL_IP": virtual_ip,
        "RUST_LOG": "info,chappy_perforator=debug,chappy=debug,rustls=error",
        "RUST_BACKTRACE": "1",
        **get_otel_env(),
    }
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps(
            {
                "role": "scheduler",
                "bucket_name": bucket_name,
                "timeout_sec": 38,
                "query": base64.b64encode(query.encode()).decode(),
                "env": env,
            }
        ).encode(),
        InvocationType="RequestResponse",
        LogType="None",
    )
    lambda_res["Payload"] = lambda_res["Payload"].read().decode()
    return (lambda_res, time.time() - start_time)


@task
def distributed(c, seed, dataset=10):
    """CREATE EXTERNAL TABLE and find out stored page data by url_host_registered_domain"""
    bucket_name = core.bucket_name(c)
    core.load_commoncrawl_index(c, dataset)
    cluster_id = rand_cluster_id()
    sql = f"""
CREATE EXTERNAL TABLE commoncrawl STORED AS PARQUET
LOCATION 's3://{bucket_name}/commoncrawl/index/n{dataset}/';
SELECT url_host_registered_domain, SUM(warc_record_length) AS stored_bytes FROM commoncrawl
GROUP BY url_host_registered_domain
ORDER BY SUM(warc_record_length) DESC
LIMIT 10;"""

    executor_count = dataset
    lambda_name = terraform_output(c, "ballista", "distributed_lambda_name")
    with ThreadPoolExecutor(max_workers=executor_count + 4) as ex:
        scheduler_fut = ex.submit(
            run_scheduler,
            lambda_name,
            bucket_name,
            seed,
            "172.28.0.1",
            sql,
            executor_count + 1,
            cluster_id,
        )
        executor_futs = []
        for i in range(executor_count):
            executor_futs.append(
                ex.submit(
                    run_executor,
                    lambda_name,
                    bucket_name,
                    seed,
                    f"172.28.0.{i+2}",
                    "172.28.0.1",
                    executor_count + 1,
                    cluster_id,
                )
            )

        scheduler_res, scheduler_duration = scheduler_fut.result()
        print(format_lambda_result("SCHEDULER", scheduler_duration, scheduler_res))
        for i in range(executor_count):
            executor_res, executor_duration = executor_futs[i].result()
            print(format_lambda_result(f"EXECUTOR{i}", executor_duration, executor_res))
