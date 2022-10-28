import base64
import os
import subprocess
from subprocess import PIPE
import logging
import pexpect
from pexpect import popen_spawn
import time
import sys
import socket
from contextlib import closing


logging.getLogger().setLevel(logging.INFO)

# Create global variables for the forked processes
process_cli = None
IS_COLD_START = True

process_config = {
    "scheduler": {
        "cmd": [
            "/opt/ballista/ballista-scheduler",
            "--sled-dir",
            "/tmp/scheduler/sled",
        ],
        "health_check_port": 50050,
    },
    "executor": {
        "cmd": ["/opt/ballista/ballista-executor"],
        "health_check_port": 50051,
    },
}


def popen_process(process_name):
    # start process
    process = subprocess.Popen(
        process_config[process_name]["cmd"],
        stdout=PIPE,
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info(f"{process_name} starts")
    # wait till the {process_name} is up and running
    c = 0
    start_time = time.time()
    while True:
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as sock:
            s = sock.connect_ex(
                ("localhost", process_config[process_name]["health_check_port"])
            )
            if s == 0:
                break
            else:
                c += 1
        timeout = time.time() - start_time
        if timeout >= 30:  # or rc is not None:
            process.terminate()
            logging.error(process.stdout.read().decode("utf-8"))
            logging.error(process.stderr.read().decode("utf-8"))
            raise Exception(
                f"{process_name} failed to start after {timeout} seconds and {c} connection tries"
            )
    logging.debug(
        f"{process_name} healthcheck passed after {timeout} seconds and {c} connection tries"
    )
    globals()[process_name] = process


def start_cli():
    global process_cli
    logging.info("cli starts")
    process_cli = popen_spawn.PopenSpawn(
        [
            "/opt/ballista/ballista-cli",
            "--host",
            "localhost",
            "--port",
            "50050",
            "--format",
            "csv",
        ]
    )
    process_cli.expect(b"\n")
    logging.debug(process_cli.before)


def init():
    for key in process_config.keys():
        popen_process(key)
    start_cli()


def check_components():
    global process_cli
    for key in process_config:
        process = globals()[key]
        status = process.poll()
        if status is not None:
            logging.debug(f"{key} failed between runs. exit_code:{s} \n Restarting...")
            popen_process(key)
    cli = process_cli.poll()
    if cli is not None:
        logging.debug(f"cli failed between runs. exit_code:{s} \n Restarting...")
        start_cli()


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    start = time.time()

    if is_cold_start:
        init()
    else:
        check_components()
    init_duration = time.time() - start

    # input parameters
    logging.debug("event: %s", event)
    src_command = base64.b64decode(event["cmd"]).decode("utf-8")

    logging.info("command: %s", src_command)
    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v

    # process_cli = subprocess.run(command, capture_output=True)
    global process_cli
    tmp_out = b""
    tmp_error = b""
    try:
        for command in src_command.split(";"):
            if command != "":
                try:
                    process_cli.sendline(command + ";")
                    logging.debug(command)
                    look_for = [
                        b"Query took",
                        pexpect.EOF,
                        b'DataFusionError\(Execution\("Table .*already exists"\)\)',
                    ]
                    i = process_cli.expect(look_for, timeout=60)
                    tmp_out = tmp_out + process_cli.before
                    if i == 0:
                        tmp_out += look_for[i]
                    elif i == 2:
                        tmp_error += look_for[i]
                        break
                    else:
                        tmp_error += "pexpect.exceptions.EOF: End Of File (EOF)"
                except pexpect.exceptions.TIMEOUT:
                    logging.debug("this command timeout flushing std_out to output")
                    tmp_out += process_cli.before
    finally:
        process_cli.expect(b"\n")
        tmp_out = tmp_out + process_cli.before + b"\n"

    cli_stdout = tmp_out.decode("utf-8")
    if tmp_error != b"":
        ret_code = 1
    else:
        ret_code = 0
    result = {
        "stdout": cli_stdout,  # process_cli.stdout,
        "stderr": tmp_error.decode("utf-8"),
        "returncode": ret_code,
        "env": event.get("env", {}),
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
            "init_duration_sec": init_duration,
        },
    }
    return result


if __name__ == "__main__":
    ballista_cmd = f"""
CREATE EXTERNAL TABLE trips01 STORED AS PARQUET
 LOCATION 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) FROM trips01
 GROUP BY payment_type;"""
    res = handler(
        {"cmd": base64.b64encode(ballista_cmd.encode("utf-8"))},
        {},
    )
    print(res)
