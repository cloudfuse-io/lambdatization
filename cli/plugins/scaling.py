"""Benchmark scaling Docker based lambdas"""

import json
import multiprocessing
import time

from common import aws, terraform_output
from invoke import task

# Set a sleep duration to make sure every invocation is alocated to a new Lambda
# container and doesn't trigger a warm start
SLEEP_DURATION = 2


def call(lambda_name):
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps({"sleep": SLEEP_DURATION}).encode(),
        InvocationType="RequestResponse",
    )
    resp_payload = lambda_res["Payload"].read().decode()
    res = json.loads(resp_payload)
    if "errorMessage" in res:
        raise Exception(res["errorMessage"])
    return res


def wait_deployment(lambda_name):
    start = time.time()
    while True:
        conf = aws("lambda").get_function_configuration(FunctionName=lambda_name)
        if conf["LastUpdateStatus"] == "Successful":
            break
        if time.time() - start > 120:
            raise Exception("Function resizing timed out")
        time.sleep(1)


def resize(lambda_name, size_mb) -> str:
    wait_deployment(lambda_name)
    aws("lambda").update_function_configuration(
        FunctionName=lambda_name, MemorySize=size_mb
    )
    wait_deployment(lambda_name)


@task
def run(c, nb=100, memory_mb=2048):
    """Run "nb" Lambdas with "memory_mb" size"""
    lambda_names = terraform_output(c, "scaling", "lambda_names").split(",")

    results = []
    for lambda_name in lambda_names:
        resize(lambda_name, memory_mb)
        with multiprocessing.Pool(nb) as pool:
            start_time = time.time()
            cold_starts = 0
            placeholder_size = None
            for res in pool.map(call, iterable=[lambda_name] * nb, chunksize=1):
                if placeholder_size is None:
                    placeholder_size = res["placeholder_size"]
                else:
                    assert placeholder_size == res["placeholder_size"]
                assert memory_mb == res["memory_limit_in_mb"]
                if res["cold_start"]:
                    cold_starts += 1
            external_duration_sec = time.time() - start_time
            res = {
                "placeholder_size": placeholder_size,
                "nb_run": nb,
                "nb_cold_start": cold_starts,
                "sleep_duration": SLEEP_DURATION,
                "external_duration_sec": external_duration_sec,
                "memory_size_mb": memory_mb,
            }
            results.append(res)

    return results
