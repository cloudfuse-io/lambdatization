import base64
import logging
import os
import socket
import subprocess
import sys
import time
import traceback
from typing import Any
from contextlib import closing

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True


class Perforator:
    def __init__(self):
        self.proc = subprocess.Popen(
            ["/opt/ballista/chappy-perforator"],
            stderr=subprocess.PIPE,
        )
        self.logs = ""
        time.sleep(0.01)

    def _load_logs(self):
        if self.logs == "":
            self.proc.terminate()
            assert self.proc.stderr is not None
            self.logs = self.proc.stderr.read().decode().strip()

    def get_logs(self) -> str:
        self._load_logs()
        return self.logs

    def log(self, log=logging.info):
        perf_logs_prefixed = "\n".join(
            [f"[PERFORATOR] {line}" for line in self.get_logs().split("\n")]
        )
        log(f"=> PERFORATOR LOGS:\n{perf_logs_prefixed}")


def wait_for_socket(process_name: str, port: int):
    c = 0
    start_time = time.time()
    while True:
        with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as sock:
            s = sock.connect_ex(("localhost", port))
            duration = time.time() - start_time
            c += 1
            if s == 0:
                msg = f"{process_name} up after {duration} secs and {c} connection attempts"
                logging.info(msg)
                break
        if duration >= 5:
            raise Exception(f"{process_name} timed out after {c} connection attempts")
        time.sleep(0.02)


def start_server(name: str, cmd: list[str], port: int):
    subprocess.Popen(
        cmd,
        stderr=sys.stderr,
        bufsize=0,
    )
    logging.info(f"{name} starting...")
    wait_for_socket(name, port)


def init_scheduler():
    cmd = [
        "/opt/ballista/ballista-scheduler",
        "--sled-dir",
        "/tmp/scheduler/sled",
        "--bind-host",
        os.environ["CHAPPY_VIRTUAL_IP"],
        "--bind-port",
        "50050",
    ]
    start_server("scheduler", cmd, 50050)


def init_executor(scheduler_ip: str):
    cmd = [
        "/opt/ballista/ballista-executor",
        "--external-host",
        os.environ["CHAPPY_VIRTUAL_IP"],
        "--bind-host",
        os.environ["CHAPPY_VIRTUAL_IP"],
        "--bind-port",
        "50051",
        "--scheduler-host",
        scheduler_ip,
        "--scheduler-port",
        "50050",
        "--concurrent-tasks",
        "1",
    ]
    start_server("executor", cmd, 50051)


def run_cli(sql: str, timeout: float) -> tuple[str, str]:
    logging.info("cli starts")
    with open("/tmp/sql_query.tmp", "w") as tmp_sql:
        tmp_sql.write(sql)
    process_cli = subprocess.Popen(
        [
            "/opt/ballista/ballista-cli",
            "--host",
            "localhost",
            "--port",
            "50050",
            "--format",
            "csv",
            "--file",
            "/tmp/sql_query.tmp",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    stdout, stderr = process_cli.communicate(input=sql.encode(), timeout=timeout)
    return stdout.decode(), stderr.decode()


def handle_event(event) -> dict[str, Any]:
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    logging.info(f"role :{event['role']}")
    timeout_sec = float(event["timeout_sec"])

    if is_cold_start:
        if event["role"] == "scheduler":
            init_scheduler()
        elif event["role"] == "executor":
            init_executor(event["scheduler_ip"])
        else:
            raise Exception(f'Unknown role {event["role"]}')

    init_duration = time.time() - start

    result = {}
    if event["role"] == "scheduler":
        query_start = time.time()
        src_command = base64.b64decode(event["query"]).decode("utf-8")
        resp, logs = run_cli(src_command, timeout_sec)
        result["resp"] = resp
        result["logs"] = logs
        result["parsed_queries"] = [src_command]
        result["query_duration_sec"] = time.time() - query_start
    elif event["role"] == "executor":
        time.sleep(timeout_sec)

    result["context"] = {
        "cold_start": is_cold_start,
        "handler_duration_sec": time.time() - start,
        "init_duration_sec": init_duration,
    }
    return result


def handler(event, context):
    """AWS Lambda handler

    event:
    - timeout_sec: float
    - env: dict
    - role: "executor" | "scheduler"
    - query: str (base64)
    - scheduler_ip: str
    """
    for key, value in event["env"].items():
        logging.info(f"{key}={value}")
        os.environ[key] = str(value)

    perforator = Perforator()
    try:
        result = handle_event(event)
    except Exception:
        result = {"exception": traceback.format_exc()}
    perforator.log()
    result["perforator_logs"] = perforator.get_logs()
    return result
