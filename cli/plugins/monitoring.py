import base64
import dynaconf
import json
import time
import common
import plugins.databend as databend
import plugins.spark as spark
import plugins.dremio as dremio
from datetime import datetime
from google.cloud import bigquery
from google.oauth2 import service_account
from invoke import task, Context

from common import REPOROOT, TF_BACKEND_VALIDATORS, auto_app_fmt

MONITORING_TFDIR = f"{REPOROOT}/infra/monitoring"
MONITORING_MODULE_DIR = f"{MONITORING_TFDIR}/bigquery"


VALIDATORS = [
    *TF_BACKEND_VALIDATORS,
    dynaconf.Validator("L12N_GCP_REGION"),
    dynaconf.Validator("L12N_GCP_PROJECT_ID"),
]


def monitoring_output(c: Context, variable):
    cmd = f"terraform -chdir={MONITORING_MODULE_DIR} output --raw {variable}"
    return c.run(cmd, hide=True).stdout


@task
def login(c):
    c.run("gcloud auth application-default login --no-launch-browser")


@task
def init(c):
    c.run(
        f"terragrunt init --terragrunt-working-dir {MONITORING_MODULE_DIR}",
    )


@task
def deploy(c, auto_approve=False):
    init(c)
    c.run(
        f"terragrunt apply {auto_app_fmt(auto_approve)} --terragrunt-working-dir {MONITORING_MODULE_DIR}",
    )


@task
def destroy(c, auto_approve=False):
    init(c)
    c.run(
        f"terragrunt apply {auto_app_fmt(auto_approve)} --terragrunt-working-dir {MONITORING_MODULE_DIR}",
    )


def send_standalone_durations(c: Context, lambda_json_output: str):
    """Read json from stdin and extracts appropriate fields to Bigquery"""
    gcp_creds = monitoring_output(c, "service_account_key")
    bigquery_table_id = monitoring_output(c, "standalone_durations_table_id")
    context = json.loads(lambda_json_output)["context"]
    client = bigquery.Client(
        credentials=service_account.Credentials.from_service_account_info(
            json.loads(base64.decodebytes(gcp_creds.encode()).decode())
        )
    )

    row = {
        "timestamp": str(datetime.now()),
        "engine": context["engine"],
        "cold_start": context["cold_start"],
        "external_duration_ms": int(context["external_duration_sec"] * 1000),
    }

    errors = client.insert_rows_json(bigquery_table_id, [row])
    if errors == []:
        print(f"Row added for {context['engine']}")
    else:
        print("Encountered errors while inserting rows: {}".format(errors))


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
        except Exception as e:
            print(e)

    while True:
        if "databend" in active_plugins:
            run_and_send_twice(databend.lambda_example)
        if "spark" in active_plugins:
            run_and_send_twice(spark.lambda_example_hive)
        if "dremio" in active_plugins:
            run_and_send_twice(dremio.lambda_example)
        time.sleep(300)
