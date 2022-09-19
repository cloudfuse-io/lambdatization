from invoke import task, Exit
import base64
import json
from common import terraform_output, aws, parse_env


CMD_HELP = {
    "cmd": "Bash commands to be executed. We recommend wrapping it with single\
 quotes to avoid unexpected interpolations",
    "env": "List of environment variables to be passed to the execution context,\
 name and values are separated by = (e.g --env BUCKET=mybucketname)",
}


def print_lambda_output(json_response: str, json_output: bool):
    if json_output:
        print(json_response)
    else:
        response = json.loads(json_response)
        print("PARSED COMMAND:")
        print(response["parsed_cmd"])
        print("\nENV:")
        print(response["env"])
        print("\nSTDOUT:")
        print(response["stdout"])
        print("\nSTDERR:")
        print(response["stderr"])
        print("\nRETURN CODE:")
        print(response["returncode"])


@task(help=CMD_HELP, iterable=["env"])
def run_lambda(c, cmd, env=[], json_output=False):
    """Run ad-hoc commands from AWS Lambda

    Prints the inputs (command / environment) and outputs (stdout, stderr, exit
    code) of the executed function to stdout."""
    lambda_name = terraform_output(c, "spark", "lambda_name")
    cmd_b64 = base64.b64encode(cmd.encode()).decode()
    lambda_res = aws("lambda").invoke(
        FunctionName=lambda_name,
        Payload=json.dumps({"cmd": cmd_b64, "env": parse_env(env)}).encode(),
        InvocationType="RequestResponse",
    )
    resp_payload = lambda_res["Payload"].read().decode()
    if "FunctionError" in lambda_res:
        # For command errors (the most likely ones), display the same object as
        # for successful results. Otherwise display the raw error payload.
        mess = resp_payload
        try:
            json_payload = json.loads(resp_payload)
            if json_payload["errorType"] == "CommandException":
                # CommandException is JSON encoded
                print_lambda_output(json_payload["errorMessage"], json_output)
                mess = ""
        except Exception:
            pass
        raise Exit(message=mess, code=1)
    print_lambda_output(resp_payload, json_output)
