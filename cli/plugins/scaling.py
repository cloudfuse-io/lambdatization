"""Benchmark scaling Docker based lambdas"""

import asyncio
import json
import random
import time

from common import AsyncAWS, aws, terraform_output, wait_deployment
from invoke import task

# Set a sleep duration to make sure every invocation is alocated to a new Lambda
# container and doesn't trigger a warm start
SLEEP_DURATION = 2


def resize(lambda_name, size_mb) -> str:
    wait_deployment(lambda_name)
    aws("lambda").update_function_configuration(
        FunctionName=lambda_name, MemorySize=size_mb
    )
    wait_deployment(lambda_name)
    response = aws("lambda").publish_version(
        FunctionName=lambda_name,
    )
    return response["Version"]


async def invoke_batch(nb, lambda_name, version, memory_mb):
    async with AsyncAWS("lambda") as s:
        start_time = time.time()
        cold_starts = 0
        placeholder_size = None
        p90 = None
        p99 = None
        error = None
        # start all invocations at once
        payload_data = json.dumps({"sleep": SLEEP_DURATION}).encode()
        tasks = asyncio.as_completed(
            [s.invoke_lambda(lambda_name, version, payload_data) for _ in range(nb)]
        )
        # iterate through results as they are generated
        for cnt, task in enumerate(tasks, start=1):
            try:
                res = await task
            except Exception as e:
                if "We currently do not have sufficient capacity" in str(e):
                    error = "insufficient_capacity"
                    break
                else:
                    raise e
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
        if error is None and cold_starts != nb:
            error = "warm_starts"
        external_duration_sec = time.time() - start_time
        return {
            "nb_run": nb,
            "memory_size_mb": memory_mb,
            "sleep_duration_sec": SLEEP_DURATION,
            "placeholder_size": placeholder_size,
            "nb_cold_start": cold_starts,
            "total_duration_sec": external_duration_sec,
            "p90_duration_sec": p90,
            "p99_duration_sec": p99,
            "error": error,
        }


@task(autoprint=True)
def run(c, nb=128, memory_mb=2048):
    """Run "nb" Lambdas with "memory_mb" size"""
    lambda_names = terraform_output(c, "scaling", "lambda_names").split(",")
    picked_lambda = random.choice(lambda_names)
    version = resize(picked_lambda, memory_mb)
    res = asyncio.run(invoke_batch(nb, picked_lambda, version, memory_mb))
    return [res]
