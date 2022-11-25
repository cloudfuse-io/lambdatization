import base64
import io
import json
import time

from botocore.exceptions import ClientError
from common import (
    AWS_REGION_VALIDATOR,
    RUNTIME_TFDIR,
    TF_BACKEND_VALIDATORS,
    active_modules,
    auto_app_fmt,
    aws,
    clean_modules,
    parse_env,
    terraform_output,
)
from invoke import Context, Exit, task

VALIDATORS = [
    *TF_BACKEND_VALIDATORS,
    AWS_REGION_VALIDATOR,
]

STEP_HELP = {"step": "A specific terragrunt module on which to perform this action"}


def active_include_dirs(c: Context) -> str:
    """The --include-dir arguments for modules activated and core modules"""
    return " ".join([f"--terragrunt-include-dir={mod}" for mod in active_modules(c)])


def docker_compose(step):
    """The docker compose command in the directory of the specified step"""
    return f"docker compose --project-directory {RUNTIME_TFDIR}/{step}/build"


## Tasks


@task
def fmt(c, fix=False):
    """Fix Terraform and Terragrunt formatting"""
    tf_fix = "" if fix else "--check"
    c.run(f"terraform fmt -recursive -diff {tf_fix}")
    tg_fix = "" if fix else "--terragrunt-check"
    c.run(f"terragrunt hclfmt {tg_fix}")


@task
def docker_login(c):
    """Login the Docker client to ECR"""
    token = aws("ecr").get_authorization_token()
    user_pass = (
        base64.b64decode(token["authorizationData"][0]["authorizationToken"])
        .decode()
        .split(":")
    )
    registry = token["authorizationData"][0]["proxyEndpoint"]
    c.run(
        f"docker login --username {user_pass[0]} --password-stdin {registry}",
        in_stream=io.StringIO(user_pass[1]),
    )


def init_step(c, step):
    """Manually run terraform init on a specific step"""
    mods = active_modules(c)
    if step not in mods:
        raise Exit(f"Step {step} not part of the active modules {mods}")
    c.run(
        f"terragrunt init --terragrunt-working-dir {RUNTIME_TFDIR}/{step}",
    )


@task(help={**STEP_HELP, "clean": clean_modules.__doc__})
def init(c, step="", clean=False):
    """Manually run terraform init on one or all steps"""
    if clean:
        clean_modules()
    if step == "":
        c.run(
            f"terragrunt run-all init {active_include_dirs(c)} --terragrunt-working-dir {RUNTIME_TFDIR}",
        )
    else:
        init_step(c, step)


def deploy_step(c, step, auto_approve=False):
    """Deploy only one step of the stack"""
    init_step(c, step)
    c.run(
        f"terragrunt apply {auto_app_fmt(auto_approve)} --terragrunt-working-dir {RUNTIME_TFDIR}/{step}",
    )


@task(help=STEP_HELP)
def deploy(c, step="", auto_approve=False):
    """Deploy all the modules associated with active plugins or a specific step"""
    if step == "":
        c.run(
            f"terragrunt run-all apply {auto_app_fmt(auto_approve)} {active_include_dirs(c)} --terragrunt-working-dir {RUNTIME_TFDIR}",
        )
    else:
        deploy_step(c, step, auto_approve)


@task(
    autoprint=True,
    help={
        "service": "The qualifier of the service this image will be used for, as specified in deploy-image"
    },
)
def current_image(c, service):
    """Get the current Lambda image URI. In case of failure, returns the error message instead of the URI."""
    repo_arn = terraform_output(c, "core", f"repository_arn")
    try:
        tags = aws("ecr").list_tags_for_resource(resourceArn=repo_arn)["tags"]
    except Exception as e:
        return str(e)
    current = next(
        (tag["Value"] for tag in tags if tag["Key"] == f"current-{service}"),
        "current-image-not-defined",
    )
    return current


@task
def build_images(c, step):
    """Build the image for the provided step"""
    c.run(f"{docker_compose(step)} build")


def deploy_image(c, service, tag):
    """Push the provided image to the core image repository"""
    ## We are using the repository tags as a key value store to flag
    ## the current image of each service. This allows a controlled
    ## version rollout in the downstream infra (lambda or fargate)
    image_url = terraform_output(c, "core", f"repository_url")
    repo_arn = terraform_output(c, "core", f"repository_arn")
    # get the digest of the current image
    try:
        current_img = current_image(c, service)
        c.run(f"docker pull {current_img}")
        old_digest = c.run(
            f"docker inspect --format='{{{{.RepoDigests}}}}' {current_img}",
            hide="out",
        ).stdout
    except:
        old_digest = "current-image-not-found"
    # get the new digest
    new_digest = c.run(
        f"docker inspect --format='{{{{.RepoDigests}}}}' {tag}",
        hide="out",
    ).stdout
    # compare old an new digests
    if old_digest == new_digest:
        print("Docker image didn't change, skipping push")
        return
    # if a change occured, push and tag the new image as current
    ecr_tag = f"{image_url}:{service}-{int(time.time())}"
    c.run(f"docker image tag {tag} {ecr_tag}")
    c.run(f"docker push {ecr_tag}")
    c.run(f"docker rmi {ecr_tag}")
    aws("ecr").tag_resource(
        resourceArn=repo_arn,
        tags=[{"Key": f"current-{service}", "Value": f"{ecr_tag}"}],
    )


@task
def push_images(c, step):
    """Push the images specified in the docker compose for that step"""
    cf_str = c.run(f"{docker_compose(step)} convert --format json", hide="out").stdout
    cf_dict = json.loads(cf_str)["services"]
    for svc in cf_dict.items():
        deploy_image(c, svc[0], svc[1]["image"])


@task
def print_image_vars(c, step, format="separate"):
    """Display the tfvars file with the image tags.

    - format="separate": The output variable name for each service is the
      service name (as defined in the docker compose file) suffixed by "_image"
    - format="list": The output variable is named "images" and images are
      provided in a list"""
    cf_str = c.run(f"{docker_compose(step)} convert --format json", hide="out").stdout
    cf_dict = json.loads(cf_str)["services"]
    if format == "separate":
        for svc_name in cf_dict.keys():
            print(f'{svc_name}_image = "{current_image(c, svc_name)}"')
    elif format == "list":
        images = {f'"{current_image(c, svc_name)}"' for svc_name in cf_dict.keys()}
        print(f'images=[{",".join(images)}]')
    else:
        raise Exit("Unknown format for print-image-vars")


def destroy_step(c, step, auto_approve=False):
    """Destroy resources of the specified step. Resources depending on it should be cleaned up first."""
    init_step(c, step)
    c.run(
        f"terragrunt destroy {auto_app_fmt(auto_approve)} --terragrunt-working-dir {RUNTIME_TFDIR}/{step}",
    )


@task(help=STEP_HELP)
def destroy(c, step="", auto_approve=False):
    """Tear down the stack of all the active plugins, or a specific step

    Note that if a module was deployed and the associated plugin was removed
    from the config afterwards, it will not be destroyed"""
    if step == "":
        c.run(
            f"terragrunt run-all destroy {auto_app_fmt(auto_approve)} {active_include_dirs(c)} --terragrunt-working-dir {RUNTIME_TFDIR}",
        )
    else:
        destroy_step(c, step, auto_approve)


QUERY_HELP = {
    "query": "SQL query to be executed. We recommend wrapping it with single\
 quotes to avoid unexpected interpolations",
}


def format_lambda_output(
    json_response: str, json_output: bool, external_duration_sec: float, engine: str
):
    response = json.loads(json_response)
    # enrich the event with the external invoke duration
    response.setdefault("context", {})
    response["context"]["external_duration_sec"] = external_duration_sec
    response["context"]["engine"] = engine
    if json_output:
        return json.dumps(response)
    else:
        output = ""
        for key in ["parsed_queries", "context", "resp", "logs"]:
            output += f"{key.upper()}\n{response.get(key, '')}\n\n"
        return output


@task(help=QUERY_HELP, autoprint=True)
def run_lambda(c, engine, query, json_output=False):
    """Run ad-hoc SQL commands

    Prints the inputs (command / environment) and outputs (stdout, stderr, exit
    code) of the executed function to stdout."""
    lambda_name = terraform_output(c, engine, "lambda_name")
    query_b64 = base64.b64encode(query.encode()).decode()
    start_time = time.time()
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps({"query": query_b64}).encode(),
        InvocationType="RequestResponse",
    )
    external_duration_sec = time.time() - start_time
    resp_payload = lambda_res["Payload"].read().decode()
    if "FunctionError" in lambda_res:
        raise Exit(message=resp_payload, code=1)
    return format_lambda_output(
        resp_payload, json_output, external_duration_sec, engine
    )


@task(autoprint=True)
def bucket_name(c):
    """Name of the core bucket with sample data"""
    return terraform_output(c, "core", "bucket_name").strip()


@task
def dockerized(c, engine):
    """Run locally the engine docker image with configs similar to the Lambda runtime"""
    # Lambda works with session credentials provided through env variables
    # We exchange the credentials provided by the user with session credentials using STS
    # Compose will pick these up and export them inside the container as Lambda would
    try:
        creds = aws("sts").get_session_token()["Credentials"]
        c.config.run.env = {
            "LAMBDA_ACCESS_KEY_ID": creds["AccessKeyId"],
            "LAMBDA_SECRET_ACCESS_KEY": creds["SecretAccessKey"],
            "LAMBDA_SESSION_TOKEN": creds["SessionToken"],
        }
    except ClientError as error:
        if (
            error.response["Error"]["Message"]
            == "Cannot call GetSessionToken with session credentials"
        ):
            creds = aws().get_credentials().get_frozen_credentials()
            c.config.run.env = {
                "LAMBDA_ACCESS_KEY_ID": creds.access_key,
                "LAMBDA_SECRET_ACCESS_KEY": creds.secret_key,
                "LAMBDA_SESSION_TOKEN": creds.token,
            }
        else:
            raise Exit(
                message=error.response.get("Error", {}).get(
                    "Message", "Unidentified error getting AWS credentials"
                ),
                code=1,
            )
    compose = f"docker compose -f {RUNTIME_TFDIR}/{engine}/build/docker-compose.yaml"
    c.run(f"{compose} down -v")
    c.run(f"{compose} build")
    c.run(f"DATA_BUCKET_NAME={bucket_name(c)} {compose} run --rm {engine}")
