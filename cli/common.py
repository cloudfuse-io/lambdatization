from functools import cache
import sys
from typing import Dict, List, Set
from flags import TRACE
from invoke import Context, Exit, Failure
import dynaconf
import json
import os
import boto3.session
import boto3
import botocore.client
import shutil


@cache
def s3_regions():
    """List all the regions where S3 is available using the AWS API"""
    return boto3.session.Session().get_available_regions("s3")


AWS_REGION_VALIDATOR = dynaconf.Validator(
    "L12N_AWS_REGION", must_exist=True, is_in=s3_regions()
)

# Path aliases
REPOROOT = os.environ["REPO_DIR"]
CURRENTDIR = os.getcwd()
TFDIR = f"{REPOROOT}/infra"


def conf(validators=[]) -> dict:
    """Load variables from both the environment and the .env file if:
    - their key is prefixed with either L12N_, TF_ or AWS_"""
    dc = dynaconf.Dynaconf(
        load_dotenv=True,
        envvar_prefix=False,
        validators=validators,
    )
    return {
        k: v
        for (k, v) in dc.as_dict().items()
        if k.startswith(("L12N_", "TF_", "AWS_"))
    }


def auto_app_fmt(val: bool) -> str:
    """Format the CLI options for auto approve"""
    if val:
        return "--terragrunt-non-interactive --auto-approve"
    else:
        return ""


def list_modules(c: Context) -> List[str]:
    """List available Terragrunt modules"""
    return [
        mod
        for mod in os.listdir(TFDIR)
        if os.path.isfile(f"{TFDIR}/{mod}/terragrunt.hcl")
    ]


def active_plugins() -> Set[str]:
    """CLI plugins activated"""
    plugin_var = conf([dynaconf.Validator("L12N_PLUGINS", default="")])["L12N_PLUGINS"]
    plugin_set = {plugin.strip() for plugin in plugin_var.split(",")}
    plugin_set.discard("")
    return plugin_set


def active_modules(c: Context) -> Set[str]:
    """Terragrunt modules activated and core modules"""
    return {*active_plugins().intersection(list_modules(c)), "core"}


def tf_version(c: Context):
    """Terraform version used by the CLI"""
    version_json = c.run("terraform version -json", hide="out").stdout
    return json.loads(version_json)["terraform_version"]


def terraform_output(c: Context, step, key) -> str:
    cmd = f"terraform -chdir={TFDIR}/{step} output --raw {key}"
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
            f"The step '{step}' was not deployed, is not up to date, "
            + f"or is improperly initialized (Terraform output '{key}' not found)",
            code=1,
        )
    return output


def AWS_REGION():
    return conf(AWS_REGION_VALIDATOR)["L12N_AWS_REGION"]


def aws(service, resource=False):
    # timeout set to 1000 to be larger than lambda max duration
    config = botocore.client.Config(retries={"max_attempts": 0}, read_timeout=1000)
    if resource:
        return boto3.resource(service, region_name=AWS_REGION(), config=config)
    else:
        return boto3.client(service, region_name=AWS_REGION(), config=config)


def parse_env(env: List[str]) -> Dict[str, str]:
    """Convert a list of "key=value" strings to a dictionary of {key: value}"""

    def split_name_val(name_val):
        env_split = name_val.split("=")
        if len(env_split) != 2:
            raise Exit(f"{name_val} should have exactly one '=' char", 1)
        return env_split[0], env_split[1]

    name_val_list = [split_name_val(v) for v in env]
    return {v[0]: v[1] for v in name_val_list}


def clean_modules():
    """Delete Terragrunt and Terragrunt cache files. This does not impact the Terraform state"""
    for path in os.listdir(TFDIR):
        if os.path.isdir(f"{TFDIR}/{path}"):
            # clean terraform cache
            tf_cache = f"{TFDIR}/{path}/.terraform"
            if os.path.isdir(tf_cache):
                print(f"deleting {tf_cache}")
                shutil.rmtree(tf_cache)
            # remove generated files
            for sub_path in os.listdir(f"{TFDIR}/{path}"):
                if sub_path.endswith(".generated.tf"):
                    generated_file = f"{TFDIR}/{path}/{sub_path}"
                    print(f"deleting {generated_file}")
                    os.remove(generated_file)
