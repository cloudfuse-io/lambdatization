import base64
import dynaconf
import sys
import json
import os
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


@task
def send_standalone_durations(c):
    """Read json from stdin and extracts appropriate fields to Bigquery"""
    stdin = sys.stdin.read()
    gcp_creds = monitoring_output(c, "service_account_key")
    bigquery_table_id = monitoring_output(c, "standalone_durations_table_id")
    context = json.loads(stdin)["context"]
    client = bigquery.Client(
        credentials=service_account.Credentials.from_service_account_info(
            base64.decodebytes(gcp_creds.encode()).decode()
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
