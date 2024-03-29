"""Utilities to collect metrics about benchmarks"""

import base64
import json
import random
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
from typing import Dict, List

import common
import dynaconf
import plugins.ballista as ballista
import plugins.clickhouse as clickhouse
import plugins.dask as dask
import plugins.databend as databend
import plugins.dremio as dremio
import plugins.scaling as scaling
import plugins.spark as spark
import plugins.trino as trino
from common import (
    AWS_REGION,
    REPOROOT,
    TF_BACKEND_VALIDATORS,
    auto_app_fmt,
    clean_modules,
    configure_tf_cache_dir,
    git_rev,
)
from google.cloud import bigquery
from google.oauth2 import service_account
from invoke import Context, Exit, task

MONITORING_TFDIR = f"{REPOROOT}/infra/monitoring"
MONITORING_MODULE_DIR = f"{MONITORING_TFDIR}/bigquery"


VALIDATORS = [
    *TF_BACKEND_VALIDATORS,
    dynaconf.Validator("L12N_GCP_REGION"),
    dynaconf.Validator("L12N_GCP_PROJECT_ID"),
]


def monitoring_output(c: Context, variable):
    cmd = f"terragrunt output --terragrunt-working-dir {MONITORING_MODULE_DIR} --raw {variable}"
    return c.run(cmd, hide=True).stdout


@task
def login(c):
    """Login to GCP"""
    c.run("gcloud auth application-default login --no-launch-browser")


@task(help={"clean": str(clean_modules.__doc__)})
def init(c, clean=False, flags=""):
    """Init the monitoring modules"""
    if clean:
        clean_modules(MONITORING_TFDIR)
    configure_tf_cache_dir()
    c.run(
        f"terragrunt init --terragrunt-working-dir {MONITORING_MODULE_DIR} {flags}",
    )


@task
def deploy(c, auto_approve=False):
    """Deploy the monitoring modules"""
    init(c)
    c.run(
        f"terragrunt apply {auto_app_fmt(auto_approve)} --terragrunt-working-dir {MONITORING_MODULE_DIR}",
    )


@task
def destroy(c, auto_approve=False):
    """Destroy the monitoring modules"""
    init(c)
    c.run(
        f"terragrunt destroy {auto_app_fmt(auto_approve)} --terragrunt-working-dir {MONITORING_MODULE_DIR}",
    )


def send(c: Context, table_output_name: str, rows: List[Dict]):
    """Enrich the events with common fields and them to the specified table

    Added fields:
    - aws_region
    - timestamp
    - revision
    - is_dirty
    - branch"""
    gcp_creds = monitoring_output(c, "service_account_key")
    bigquery_table_id = monitoring_output(c, table_output_name)
    client = bigquery.Client(
        credentials=service_account.Credentials.from_service_account_info(
            json.loads(base64.decodebytes(gcp_creds.encode()).decode())
        )
    )
    rev = git_rev(c)
    for row in rows:
        row["timestamp"] = str(datetime.now())
        row["aws_region"] = AWS_REGION()
        row["revision"] = rev.revision
        row["is_dirty"] = rev.is_dirty
        row["branch"] = rev.branch
    errors = client.insert_rows_json(bigquery_table_id, rows)
    if errors == []:
        print(f"{len(rows)} row(s) added, first row: {json.dumps(rows[0])}")
    else:
        print(f"Errors while inserting row(s): {errors}")


def send_standalone_durations(c: Context, lambda_json_output: str):
    """Read json from stdin and extracts appropriate fields to Bigquery"""
    context = json.loads(lambda_json_output)["context"]
    row = {
        "engine": context["engine"],
        "cold_start": context["cold_start"],
        "external_duration_ms": int(context["external_duration_sec"] * 1000),
    }
    send(c, "standalone_durations_table_id", [row])


def send_scaling_duration(c: Context, durations: List[Dict]):
    def _map(optional, func):
        if optional is None:
            return None
        return func(optional)

    rows = []
    for dur in durations:
        sleep_dur_ms = _map(dur["sleep_duration_sec"], lambda x: int(x * 1000))
        total_dur_ms = _map(dur["total_duration_sec"], lambda x: int(x * 1000))
        p90_dur_ms = _map(dur["p90_duration_sec"], lambda x: int(x * 1000))
        p99_dur_ms = _map(dur["p99_duration_sec"], lambda x: int(x * 1000))
        ph_size_mb = _map(dur["placeholder_size"], lambda x: int(x / 10**6))
        row = {
            "sleep_duration_ms": sleep_dur_ms,
            "total_duration_ms": total_dur_ms,
            "p90_duration_ms": p90_dur_ms,
            "p99_duration_ms": p99_dur_ms,
            "placeholder_size_mb": ph_size_mb,
            "nb_run": dur["nb_run"],
            "nb_cold_start": dur["nb_cold_start"],
            "memory_size_mb": dur["memory_size_mb"],
        }
        rows.append(row)
    send(c, "scaling_durations_table_id", rows)


@task
def bench_cold_warm(c):
    """Run each engine twice in a row on different data to compare cold and warm start"""
    active_plugins = common.active_plugins()

    def run_and_send_twice(example):
        try:
            res1 = example(c, json_output=True, month="01")
            send_standalone_durations(c, res1)
            res2 = example(c, json_output=True, month="02")
            send_standalone_durations(c, res2)
        except Exit as e:
            print(f"Execution failure: {e.message}")
        except Exception as e:
            print(f"Execution failure: {e}")

    with ThreadPoolExecutor(max_workers=5) as e:
        if "trino" in active_plugins:
            e.submit(run_and_send_twice, trino.lambda_example)
        if "spark" in active_plugins:
            e.submit(run_and_send_twice, spark.lambda_example_hive)
        if "dremio" in active_plugins:
            e.submit(run_and_send_twice, dremio.lambda_example)
        if "databend" in active_plugins:
            e.submit(run_and_send_twice, databend.lambda_example)
        if "dask" in active_plugins:
            e.submit(run_and_send_twice, dask.lambda_example)
        if "ballista" in active_plugins:
            e.submit(run_and_send_twice, ballista.lambda_example)
        if "clickhouse" in active_plugins:
            e.submit(run_and_send_twice, clickhouse.lambda_example)


@task
def bench_scaling(c, nb_invocations=64):
    """Run benchmarks to assess how AWS scales Docker based Lambdas"""
    for memory_mb in random.sample([2048, 4096, 8192], k=3):
        result = scaling.run(c, nb=nb_invocations, memory_mb=memory_mb)
        send_scaling_duration(c, result)
