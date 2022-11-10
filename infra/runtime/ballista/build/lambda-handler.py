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


def wait_for_socket(process_name):
    c = 0
    start_time = time.time()
    while True:
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as sock:
            s = sock.connect_ex(
                ("localhost", process_config[process_name]["health_check_port"])
            )
            c += 1
            if s == 0:
                break
        duration = time.time() - start_time
        if duration >= 30:
            raise Exception(f"{process_name} timed out after {c} connection attempts")
        time.sleep(0.01)
    logging.info(f"{process_name} up after {duration} secs and {c} connection attempts")


def popen_process(process_name):
    # start process
    process = subprocess.Popen(
        process_config[process_name]["cmd"],
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info(f"{process_name} starts")
    # wait till the {process_name} is up and running
    wait_for_socket(process_name)
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
    process_cli.expect(b"Ballista CLI v[0-9.]*")
    logging.debug(process_cli.before)


def init():
    for key in process_config.keys():
        popen_process(key)
    start_cli()


def clean(string):
    return "\n".join([x for x in string.splitlines() if not x.startswith("[")])


def query(src_command):
    global process_cli
    tmp_out = b""
    tmp_error = b""
    parsed_queries = ""
    try:
        for command in src_command.split(";"):
            if command != "":
                try:
                    process_cli.sendline(command + ";")
                    logging.debug(command)
                    parsed_queries += command
                    look_for = [
                        b"Query took",
                        pexpect.EOF,
                        b"DataFusionError\(",
                        b"Invalid statement",
                    ]
                    i = process_cli.expect(look_for, timeout=60)
                    tmp_out = tmp_out + process_cli.before
                    if i == 0:
                        tmp_out += look_for[i]
                    else:
                        if i == 1:
                            tmp_error += "pexpect.exceptions.EOF: End Of File (EOF)"
                            process_cli.kill(9)
                            process_cli.wait()
                        else:
                            tmp_error += look_for[i]
                            process_cli.expect([b"\n"], timeout=1)
                            tmp_error += process_cli.before
                        raise Exception(tmp_error.decode("utf-8"))
                except pexpect.exceptions.TIMEOUT:
                    logging.debug("this command timeout flushing std_out to output")
                    tmp_out += process_cli.before
                    tmp_error += f"command timeout: \n {command}".encode("utf-8")
                    raise Exception(tmp_error.decode("utf-8"))
    finally:
        if tmp_error != b"" or tmp_out != b"":
            if i == 0:
                process_cli.expect(b"\n")
                tmp_out = tmp_out + process_cli.before + b"\n"
            if i > 0:
                logging.error(tmp_error.decode("utf-8"))
    return tmp_out.decode("utf-8")


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    start = time.time()

    if is_cold_start:
        init()

    init_duration = time.time() - start

    # input parameters
    logging.debug("event: %s", event)
    src_command = base64.b64decode(event["cmd"]).decode("utf-8")

    logging.info("command: %s", src_command)

    resp = query(src_command)

    result = {
        "resp": resp,
        "parsed_queries": src_command,
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
