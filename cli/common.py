import json
import os
import shutil
import sys
import time
from dataclasses import dataclass
from functools import cache
from pathlib import Path
from typing import List, Set

import aiohttp
import boto3
import botocore.client
import dynaconf
from botocore.auth import SigV4Auth
from botocore.awsrequest import AWSRequest
from flags import TRACE
from invoke import Context, Exit, Failure

# Validate and provide defaults for the terraform state backend configuration
TF_BACKEND_VALIDATORS = [
    dynaconf.Validator("TF_STATE_BACKEND", default="local", is_in=["local", "cloud"]),
    dynaconf.Validator("TF_WORKSPACE_PREFIX", default=""),
    # if we use tf cloud as backend, the right variables must be configured
    dynaconf.Validator("TF_STATE_BACKEND", ne="cloud")
    | (
        dynaconf.Validator("TF_ORGANIZATION", must_exist=True, ne="")
        & dynaconf.Validator("TF_API_TOKEN", must_exist=True, ne="")
    ),
]


@cache
def s3_regions():
    """List all the regions where S3 is available using the AWS API"""
    return boto3.session.Session().get_available_regions("s3")


AWS_REGION_VALIDATOR = dynaconf.Validator(
    "L12N_AWS_REGION", default="eu-west-1", is_in=s3_regions()
)

# Path aliases
REPOROOT = os.environ["REPO_DIR"]
CALLING_DIR = os.environ["CALLING_DIR"]
RUNTIME_TFDIR = f"{REPOROOT}/infra/runtime"
DOCKERDIR = f"{REPOROOT}/docker"


@dataclass
class GitRev:
    revision: str
    is_dirty: bool
    branch: str


def git_rev(c: Context) -> GitRev:
    try:
        revision = c.run(
            f"cd {REPOROOT}; git rev-parse --short HEAD", hide=True
        ).stdout.strip()
    except Exception:
        revision = "unknown"
    try:
        c.run(f"cd {REPOROOT}; git diff --quiet", hide=True)
        dirty = False
    except Exception:
        dirty = True
    try:
        branch = c.run(
            f"cd {REPOROOT}; git rev-parse --abbrev-ref HEAD", hide=True
        ).stdout.strip()
    except Exception:
        branch = "unknown"
    return GitRev(revision, dirty, branch)


def conf(validators=[]) -> dict:
    """Load variables from the environment if:
    - their key is prefixed with either L12N_, TF_ or AWS_"""
    assert isinstance(
        validators, list
    ), "validators should be a list of dynaconf.Validator"
    dc = dynaconf.Dynaconf(
        # dotenv file is loaded by l12n-shell
        load_dotenv=False,
        envvar_prefix=False,
        validators=validators,
    )
    return {
        k: str(v)
        for (k, v) in dc.as_dict().items()
        if k.startswith(("L12N_", "TF_", "AWS_"))
    }


def auto_app_fmt(val: bool) -> str:
    """Format the CLI options for auto approve"""
    if val:
        return "--terragrunt-non-interactive --auto-approve"
    else:
        return ""


def list_modules(module_dir) -> List[str]:
    """List available Terragrunt modules"""
    return [
        mod
        for mod in os.listdir(module_dir)
        if os.path.isfile(f"{module_dir}/{mod}/terragrunt.hcl")
    ]


def active_plugins() -> Set[str]:
    """CLI plugins activated"""
    plugin_var = conf([dynaconf.Validator("L12N_PLUGINS", default="")])["L12N_PLUGINS"]
    plugin_set = {plugin.strip() for plugin in plugin_var.split(",")}
    plugin_set.discard("")
    return plugin_set


def active_modules(module_dir) -> Set[str]:
    """Terragrunt modules activated and core modules"""
    return {*active_plugins().intersection(list_modules(module_dir)), "core"}


def tf_version(c: Context):
    """Terraform version used by the CLI"""
    version_json = c.run("terraform version -json", hide="out").stdout
    return json.loads(version_json)["terraform_version"]


def terraform_output(c: Context, module, key) -> str:
    cmd = f"terragrunt output --terragrunt-working-dir {RUNTIME_TFDIR}/{module} --raw {key}"
    try:
        output = c.run(
            cmd,
            hide=True,
            # avoid unintentionally capturing stdin
            in_stream=False,
        ).stdout
        # `terraform output` sometimes raises errors, sometimes only prints
        # warnings, according to the actual output state. Here, we streamline
        # both cases into a single exit message.
        if "No outputs found" in output:
            raise Exit(output)
    except Failure as e:
        _, err = e.streams_for_display()
        if TRACE:
            print(cmd, file=sys.stderr)
            print(err.strip(), file=sys.stderr)
        raise Exit(
            f"The module '{module}' was not deployed, is not up to date, "
            + f"or is improperly initialized (Terraform output '{key}' not found)",
            code=1,
        )
    return output


def AWS_REGION() -> str:
    return conf([AWS_REGION_VALIDATOR])["L12N_AWS_REGION"]


def aws(service=None):
    # timeout set to 1000 to be larger than lambda max duration
    if service is None:
        return boto3.Session()
    else:
        config = botocore.client.Config(retries={"max_attempts": 0}, read_timeout=1000)
        return boto3.client(service, region_name=AWS_REGION(), config=config)


def clean_modules(mod_dir):
    """Delete Terragrunt and Terragrunt cache files. This does not impact the Terraform state"""
    for path in os.listdir(mod_dir):
        if os.path.isdir(f"{mod_dir}/{path}"):
            # clean terraform cache
            tf_cache = f"{mod_dir}/{path}/.terraform"
            if os.path.isdir(tf_cache):
                print(f"deleting {tf_cache}")
                shutil.rmtree(tf_cache)
            # remove generated files
            for sub_path in os.listdir(f"{mod_dir}/{path}"):
                if sub_path.endswith(".generated.tf"):
                    generated_file = f"{mod_dir}/{path}/{sub_path}"
                    print(f"deleting {generated_file}")
                    os.remove(generated_file)


def configure_tf_cache_dir():
    """Configure a directory from which TF can reuse the providers"""
    cache_dir = f"{CALLING_DIR}/.terraform/data"
    Path(cache_dir).mkdir(parents=True, exist_ok=True)
    os.environ.update({"TF_PLUGIN_CACHE_DIR": cache_dir})


def format_lambda_output(json_response: str, json_output: bool, **context):
    response = json.loads(json_response)
    # enrich the event with the external invoke duration
    for key, value in context.items():
        response.setdefault("context", {})
        response["context"][key] = value

    if json_output:
        return json.dumps(response)
    else:
        output = ""
        for key, value in sorted(response.items()):
            output += f"{key.upper()}\n{value}\n\n"
        return output


class AsyncAWS:
    """A helper to write async queries to AWS

    This is a low level function that requires manually writing the HTTP queries.
    Should be used with the async context manager.
    The service endpoint"""

    def __init__(self, service_name, region_name=AWS_REGION()):
        # No need to specify a region as we only use the session to get the
        # frozen credentials
        session = boto3.Session(region_name=region_name)
        credentials = session.get_credentials()
        self.creds = credentials.get_frozen_credentials()
        self.region = region_name
        self.service_name = service_name
        self.endpoint = f"https://{service_name}.{region_name}.amazonaws.com"

    async def __aenter__(self):
        self.aiohttp_session = aiohttp.ClientSession()
        return self

    async def __aexit__(self, *args):
        await self.aiohttp_session.close()

    async def aws_request(
        self, method, path, data=None, params=None, headers=None
    ) -> aiohttp.ClientResponse:
        url = f"{self.endpoint}{path}"
        request = AWSRequest(
            method=method, url=url, data=data, params=params, headers=headers
        )
        SigV4Auth(self.creds, self.service_name, self.region).add_auth(request)
        async with aiohttp.ClientSession() as session:
            async with session.request(
                method, url, headers=dict(request.headers), data=data
            ) as resp:
                await resp.read()
            return resp


def wait_deployment(lambda_name):
    """Wait for a the function to be successfully deployed"""
    start = time.time()
    while True:
        conf = aws("lambda").get_function_configuration(FunctionName=lambda_name)
        if conf["LastUpdateStatus"] == "Successful":
            break
        if time.time() - start > 120:
            raise Exception("Function deployment timed out")
        time.sleep(1)


## Task management


class FargateService:
    """A service with a single task running"""

    def __init__(self, fargate_cluster_name, service_name, task_family):
        self.cluster = fargate_cluster_name
        self.service_name = service_name
        self.task_family = task_family

    def get_task_id(self, max_wait_time_sec=0, start_time=None):
        """Get the task id for this service. If no server is running, it waits
        until max_wait_time_sec for a new server to be started."""
        if start_time == None:
            start_time = time.time()
        while True:
            task_res = aws("ecs").list_tasks(
                family=self.task_family, cluster=self.cluster
            )
            nb_vast_tasks = len(task_res["taskArns"])
            if nb_vast_tasks == 1:
                task_id = task_res["taskArns"][0].split("/")[-1]
                return task_id
            if nb_vast_tasks > 1:
                raise Exit(f"{nb_vast_tasks} tasks running", 1)
            if max_wait_time_sec == 0:
                raise Exit(f"No task running", 8)
            if time.time() - start_time > max_wait_time_sec:
                raise Exit(f"Task timed out", 1)
            time.sleep(1)

    def _task_desc(self, task_arn):
        return aws("ecs").describe_tasks(cluster=self.cluster, tasks=[task_arn])[
            "tasks"
        ][0]

    def service_status(self):
        """Get the status of the service and the associated task"""
        # describe task
        task_res = aws("ecs").list_tasks(family=self.task_family, cluster=self.cluster)
        nb_vast_tasks = len(task_res["taskArns"])
        if nb_vast_tasks == 1:
            task_status = self._task_desc(task_res["taskArns"][0])["lastStatus"]
        # describe service
        srv_res = aws("ecs").describe_services(
            cluster=self.cluster, services=[self.service_name]
        )
        desired_tasks = srv_res["services"][0]["desiredCount"]

        # state machine
        if nb_vast_tasks > 1:
            return f"Unexpected number of tasks: {nb_vast_tasks}"
        if desired_tasks == 1 and nb_vast_tasks == 1 and task_status == "RUNNING":
            return "Service running"
        if desired_tasks == 1:
            if nb_vast_tasks == 0:
                task_status = "no task running"
            else:
                task_status = f"task status: {task_status}"
            return f"Service starting... ({task_status})"
        if desired_tasks == 0 and nb_vast_tasks == 1:
            return f"Service stopping... (task status: {task_status})"
        if desired_tasks == 0:
            return "Service stopped"

    def describe_task(self):
        """Describe the running tasks, erroring out if there is not exactly one task"""
        task_res = aws("ecs").list_tasks(family=self.task_family, cluster=self.cluster)
        nb_vast_tasks = len(task_res["taskArns"])
        if nb_vast_tasks == 0:
            raise Exit("No task running")
        if nb_vast_tasks > 1:
            raise Exit("{nb_vast_tasks} tasks running")
        else:
            desc = self._task_desc(task_res["taskArns"][0])
            return desc

    def _wait_for_status(
        self, task_id, target_status: str | List[str], timeout, start_time
    ):
        print(f"Waiting for task {task_id} to reach status {target_status}:")
        previous_status = ""
        while time.time() - start_time < timeout:
            status = self._task_desc(task_id)["lastStatus"]
            if previous_status != status:
                print(f"-> {status}")
                previous_status = status
            if status in target_status:
                return
            time.sleep(0.5)
        raise Exit("Timed out")

    def start_service(self, timeout=300):
        """Start the service. Noop if it is already running"""
        start_time = time.time()
        print("Starting service...")
        aws("ecs").update_service(
            cluster=self.cluster, service=self.service_name, desiredCount=1
        )
        task_id = self.get_task_id(timeout, start_time)
        self._wait_for_status(task_id, "RUNNING", timeout, start_time)

    def _stop_task(self, timeout, start_time):
        "Stop the current running task in this service."
        task_id = self.get_task_id()
        print(f"Calling stop on task {task_id}...")
        aws("ecs").stop_task(task=task_id, cluster=self.cluster)
        self._wait_for_status(
            task_id, ["DEPROVISIONING", "STOPPED"], timeout, start_time
        )

    def stop_service(self, timeout=200):
        """Stop the service and its task"""
        start_time = time.time()
        print("Stopping service...")
        aws("ecs").update_service(
            cluster=self.cluster, service=self.service_name, desiredCount=0
        )
        self._stop_task(timeout, start_time)
