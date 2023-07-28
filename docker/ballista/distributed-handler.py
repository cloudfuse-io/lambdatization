import base64
import logging
import os
import socket
import subprocess
import tempfile
import time
import traceback
from contextlib import closing
from dataclasses import dataclass
from typing import Any

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True


class Perforator:
    def __init__(self, bin_path):
        self.tmp_file = tempfile.NamedTemporaryFile(mode="w+", delete=True)
        self.proc = subprocess.Popen(
            [bin_path],
            stderr=self.tmp_file,
        )
        self.logs = ""

    def _load_logs(self):
        if self.logs == "":
            self.proc.terminate()
            try:
                self.proc.communicate(timeout=5)
                logging.info("Perforator successfully terminated")
            except subprocess.TimeoutExpired:
                logging.error("Perforator could not terminate properly")
                self.proc.kill()
                self.proc.communicate()
            self.tmp_file.seek(0)
            self.logs = self.tmp_file.read().strip()
            self.tmp_file.close()

    def get_logs(self) -> str:
        self._load_logs()
        return self.logs

    def log(self, log=logging.info):
        perf_logs_prefixed = "\n".join(
            [f"[PERFORATOR] {line}" for line in self.get_logs().split("\n")]
        )
        log(f"=> PERFORATOR LOGS:\n{perf_logs_prefixed}")


def wait_for_socket(process_name: str, port: int) -> bool:
    """Return false as soon as a TCP server is reachable on the provided port.
    Return true if it isn't reachable within timeout"""
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
            logging.error(f"{process_name} timed out after {c} connection attempts")
            return True
        time.sleep(0.02)
    return False


@dataclass
class ProcessResult:
    process: subprocess.Popen[bytes]
    start_timeout: bool


def start_server(name: str, cmd: list[str], port: int) -> ProcessResult:
    srv_proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )
    logging.info(f"{name} starting...")
    is_timeout = wait_for_socket(name, port)
    return ProcessResult(srv_proc, is_timeout)


def init_scheduler() -> ProcessResult:
    cmd = [
        "/opt/ballista/ballista-scheduler",
        "--sled-dir",
        "/tmp/scheduler/sled",
        "--bind-host",
        "127.0.0.1",
        "--bind-port",
        "50050",
        "--log-level-setting",
        "DEBUG",
    ]
    return start_server("scheduler", cmd, 50050)


def init_executor(scheduler_ip: str) -> ProcessResult:
    cmd = [
        "/opt/ballista/ballista-executor",
        "--external-host",
        os.environ["CHAPPY_VIRTUAL_IP"],
        "--bind-host",
        "127.0.0.1",
        "--bind-port",
        "50051",
        "--scheduler-host",
        scheduler_ip,
        "--scheduler-port",
        "50050",
        "--concurrent-tasks",
        "1",
        "--log-level-setting",
        "DEBUG",
    ]
    return start_server("executor", cmd, 50051)


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
    try:
        stdout, stderr = process_cli.communicate(input=sql.encode(), timeout=timeout)
    except subprocess.TimeoutExpired as e:
        stdout = e.stdout
        stderr = e.stderr
    assert stdout is not None, "not None if PIPE specified"
    assert stderr is not None, "not None if PIPE specified"
    return stdout.decode(), stderr.decode()


def stop_server(proc: subprocess.Popen[bytes]) -> tuple[str, str]:
    if proc.poll() is None:
        proc.kill()
    assert proc.stdout is not None
    assert proc.stderr is not None
    return (proc.stdout.read().decode(), proc.stderr.read().decode())


def handle_event(event) -> dict[str, Any]:
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if not is_cold_start:
        raise Exception(f"Only cold starts supported")

    logging.info(f"role :{event['role']}")
    timeout_sec = float(event["timeout_sec"])

    if event["role"] == "scheduler":
        srv_proc = init_scheduler()
    elif event["role"] == "executor":
        srv_proc = init_executor(event["scheduler_ip"])
    else:
        raise Exception(f'Unknown role {event["role"]}')

    init_duration = time.time() - start

    result = {}
    if srv_proc.start_timeout:
        result["startup_timeout"] = "true"
        (srv_stdout, srv_stderr) = stop_server(srv_proc.process)
    elif event["role"] == "scheduler":
        query_start = time.time()
        src_command = base64.b64decode(event["query"]).decode("utf-8")
        resp, logs = run_cli(src_command, timeout_sec)
        (srv_stdout, srv_stderr) = stop_server(srv_proc.process)
        result["cli_resp"] = resp
        result["cli_logs"] = logs
        result["parsed_queries"] = [src_command]
        result["query_duration_sec"] = time.time() - query_start
    elif event["role"] == "executor":
        time.sleep(timeout_sec)
        (srv_stdout, srv_stderr) = stop_server(srv_proc.process)
    else:
        raise Exception(f'Unknown role {event["role"]}')

    result["srv_stdout"] = srv_stdout
    result["srv_stderr"] = srv_stderr

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

    perforator = Perforator("/opt/ballista/chappy-perforator")
    try:
        result = handle_event(event)
    except Exception:
        result = {"exception": traceback.format_exc()}
    result["perforator_logs"] = perforator.get_logs()
    return result
