import base64
import os
import subprocess
from subprocess import PIPE
import logging
import pexpect
from pexpect import popen_spawn
import time
import sys


logging.getLogger().setLevel(logging.INFO)

# Create global variables for the forked processes
process_s = None
process_e = None
process_cli = None
IS_COLD_START = True


def start_scheduler():
    # start scheduler
    global process_s
    process_s = subprocess.Popen(
        ["/opt/ballista/ballista-scheduler", "--sled-dir", "/tmp/scheduler/sled"],
        stdout=PIPE,
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info("scheduler starts")
    time.sleep(1)
    # wait till the scheduler is up and running
    if process_s.poll() is None:
        return_code = 1
        timeout = 0
        while return_code:
            res = subprocess.run(
                ["nc", "-z", "localhost", "50050"], capture_output=True
            )
            return_code = res.returncode
            logging.debug(res)
            time.sleep(1)
            # rc = process_s.poll()
            timeout += 1
            if timeout >= 30:  # or rc is not None:
                process_s.terminate()
                logging.error(process_s.stdout.read().decode("utf-8"))
                logging.error(process_s.stderr.read().decode("utf-8"))
                raise Exception(f"scheduler failed to start after {timeout} seconds")
    else:
        logging.error(process_s.stdout.read().decode("utf-8"))
        raise Exception(
            f"scheduler failed to init stderror: {process_s.stderr.read().decode('utf-8')}"
        )


def start_executor():
    # start executor
    global process_e
    process_e = subprocess.Popen(
        ["/opt/ballista/ballista-executor"],
        stdout=PIPE,
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info("executor starts")
    # wait till the executor is up and running
    if process_s.poll() is None:
        return_code = 1
        timeout = 0
        while return_code:
            res = subprocess.run(
                ["nc", "-z", "localhost", "50051"], capture_output=True
            )
            return_code = res.returncode
            logging.debug(res)
            time.sleep(1)
            timeout += 1
            if timeout >= 30:
                process_e.terminate()
                logging.error(process_e.stdout.read().decode("utf-8"))
                logging.error(process_e.stderr.read().decode("utf-8"))
                process_s.terminate()
                logging.error(process_s.stdout.read().decode("utf-8"))
                logging.error(process_s.stderr.read().decode("utf-8"))
                raise Exception(f"executor failed to start after {timeout} seconds")
    else:
        logging.error(process_e.stdout.read().decode("utf-8"))
        logging.error(process_s.stdout.read().decode("utf-8"))
        raise Exception(
            f"executor failed to init stderror: {process_e.stderr.read().decode('utf-8')}"
        )


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
    start_scheduler()
    start_executor()
    start_cli()


def check_components():
    global process_cli
    global process_s
    global process_e
    s = process_s.poll()
    if s is not None:
        logging.debug(f"scheduler failed between runs. exit_code:{s} \n Restarting...")
        start_scheduler()
    e = process_e.poll()
    if e is not None:
        logging.debug(f"executor failed between runs. exit_code:{s} \n Restarting...")
        start_executor()
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
