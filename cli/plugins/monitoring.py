import dynaconf
from invoke import task

from common import MONITORING_TFDIR, TF_BACKEND_VALIDATORS, auto_app_fmt


MONITORING_MODULE_DIR = f"{MONITORING_TFDIR}/bigquery"


VALIDATORS = [
    *TF_BACKEND_VALIDATORS,
    dynaconf.Validator("L12N_GCP_REGION"),
    dynaconf.Validator("L12N_GCP_PROJECT_ID"),
]


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
    print("=> To use the created service account (BigQuery editor) run:")
    print("""echo "L12N_GCP_B64_KEY=$(l12n monitoring.key)" >> .env """)


@task
def destroy(c, auto_approve=False):
    init(c)
    c.run(
        f"terragrunt apply {auto_app_fmt(auto_approve)} --terragrunt-working-dir {mod_dir}",
    )


@task(autoprint=True)
def key(c):
    cmd = f"terraform -chdir={MONITORING_MODULE_DIR} output --raw service_account_key"
    return c.run(cmd, hide=True).stdout
