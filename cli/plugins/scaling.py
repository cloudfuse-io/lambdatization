"""Benchmark scaling Docker based lambdas"""

import asyncio
import json
import random
import time

from common import AsyncAWS, aws, terraform_output
from invoke import task

# Set a sleep duration to make sure every invocation is alocated to a new Lambda
# container and doesn't trigger a warm start
SLEEP_DURATION = 2


def wait_deployment(lambda_name):
    start = time.time()
    while True:
        conf = aws("lambda").get_function_configuration(FunctionName=lambda_name)
        if conf["LastUpdateStatus"] == "Successful":
            break
        if time.time() - start > 120:
            raise Exception("Function resizing timed out")
        time.sleep(1)


def resize(lambda_name, size_mb):
    wait_deployment(lambda_name)
    aws("lambda").update_function_configuration(
        FunctionName=lambda_name, MemorySize=size_mb
    )
    wait_deployment(lambda_name)


async def invoke(lambda_name: str, session: AsyncAWS):
    resp = await session.aws_request(
        method="POST",
        path=f"/2015-03-31/functions/{lambda_name}/invocations",
        data=json.dumps({"sleep": SLEEP_DURATION}).encode(),
        headers={
            "X-Amz-Invocation-Type": "RequestResponse",
            "X-Amz-Log-Type": "None",
        },
    )
    resp.raise_for_status()
    res = json.loads(await resp.text())
    if "errorMessage" in res:
        raise Exception(res["errorMessage"])
    return res


async def invoke_batch(nb, lambda_name, memory_mb):
    async with AsyncAWS("lambda") as s:
        start_time = time.time()
        cold_starts = 0
        placeholder_size = None
        p90 = None
        p99 = None
        # start all invocations at once
        tasks = asyncio.as_completed([invoke(lambda_name, s) for _ in range(nb)])
        # iterate through results as they are generated
        for cnt, task in enumerate(tasks):
            res = await task
            if placeholder_size is None:
                placeholder_size = res["placeholder_size"]
            else:
                assert placeholder_size == res["placeholder_size"]
            assert memory_mb == res["memory_limit_in_mb"]
            if res["cold_start"]:
                cold_starts += 1
            # record quantiles when appropriate
            if cnt == int(0.9 * nb):
                p90 = time.time() - start_time
            elif cnt == int(0.99 * nb):
                p99 = time.time() - start_time

        external_duration_sec = time.time() - start_time
        return {
            "placeholder_size": placeholder_size,
            "nb_run": nb,
            "nb_cold_start": cold_starts,
            "sleep_duration_sec": SLEEP_DURATION,
            "total_duration_sec": external_duration_sec,
            "p90_duration_sec": p90,
            "p99_duration_sec": p99,
            "memory_size_mb": memory_mb,
        }


@task(autoprint=True)
def run(c, nb=100, memory_mb=2048):
    """Run "nb" Lambdas with "memory_mb" size"""
    lambda_names = terraform_output(c, "scaling", "lambda_names").split(",")
    random.shuffle(lambda_names)

    results = []
    for lambda_name in lambda_names:
        resize(lambda_name, memory_mb)
        res = asyncio.run(invoke_batch(nb, lambda_name, memory_mb))
        results.append(res)

    return results
