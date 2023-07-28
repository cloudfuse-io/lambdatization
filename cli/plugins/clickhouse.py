"""Clickhouse on AWS Lambda"""

import base64
import json
import time
from concurrent.futures import ThreadPoolExecutor

import core
from common import (
    AWS_REGION,
    OTEL_VALIDATORS,
    aws,
    format_lambda_output,
    get_otel_env,
    rand_cluster_id,
    terraform_output,
)
from invoke import Exit, task

VALIDATORS = OTEL_VALIDATORS


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type with direct FROM s3()"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3('https://{core.bucket_name(c)}.s3.{AWS_REGION()}.amazonaws.com/nyc-taxi/2019/{month}/*', 'Parquet')
GROUP BY payment_type"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "clickhouse", sql, json_output=json_output)


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


def run_node(
    lambda_name: str,
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
        "RUST_LOG": "info,chappy_perforator=debug,chappy=trace,rustls=error",
        "RUST_BACKTRACE": "1",
        **get_otel_env(),
    }
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps(
            {
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
def distributed(c, seed, dataset=10, nodes=5):
    """CREATE EXTERNAL TABLE and find out stored page data by url_host_registered_domain"""
    bucket_name = core.bucket_name(c)
    core.load_commoncrawl_index(c, dataset)
    cluster_id = rand_cluster_id()
    sql = f"""
SELECT url_host_registered_domain, sum(warc_record_length) AS stored_bytes
FROM s3Cluster('cloudfuse_cluster', 'https://{bucket_name}.s3.{AWS_REGION()}.amazonaws.com/commoncrawl/index/n{dataset}/crawl=CC-MAIN-2023-14/subset=warc/*', 'Parquet')
GROUP BY url_host_registered_domain
ORDER BY sum(warc_record_length) DESC
LIMIT 10;
"""

    lambda_name = terraform_output(c, "clickhouse", "distributed_lambda_name")
    with ThreadPoolExecutor(max_workers=nodes + 4) as ex:
        node_futs = []
        for i in range(1, nodes + 1):
            node_futs.append(
                ex.submit(
                    run_node,
                    lambda_name,
                    seed,
                    f"172.28.0.{i}",
                    sql if i == nodes else "",
                    nodes,
                    cluster_id,
                )
            )

        for i in range(0, nodes):
            executor_res, executor_duration = node_futs[i].result()
            print(format_lambda_result(f"NODE-{i+1}", executor_duration, executor_res))
