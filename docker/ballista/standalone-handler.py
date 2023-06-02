import base64
import logging
import os
import socket
import subprocess
import time
import traceback
from contextlib import closing
from dataclasses import dataclass
from typing import Any

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True


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

    # TODO this restarts the servers at each time, which degrades warm starts
    sched_proc = init_scheduler()
    exec_proc = init_executor("127.0.0.1")

    init_duration = time.time() - start
    cli_timeout_sec = 60

    result = {}
    if sched_proc.start_timeout or exec_proc.start_timeout:
        result["startup_timeout"] = "true"
        sched_stdout, sched_stderr = stop_server(sched_proc.process)
        exec_stdout, exec_stderr = stop_server(exec_proc.process)
    else:
        query_start = time.time()
        src_command = base64.b64decode(event["query"]).decode("utf-8")
        cli_stdout, cli_stderr = run_cli(src_command, cli_timeout_sec)
        result["query_duration_sec"] = time.time() - query_start
        sched_stdout, sched_stderr = stop_server(sched_proc.process)
        result["cli_resp"] = cli_stdout
        result["cli_logs"] = cli_stderr
        result["parsed_queries"] = [src_command]
        exec_stdout, exec_stderr = stop_server(exec_proc.process)

    result["sched_stdout"] = sched_stdout
    result["sched_stderr"] = sched_stderr
    result["exec_stdout"] = exec_stdout
    result["exec_stderr"] = exec_stderr

    result["context"] = {
        "cold_start": is_cold_start,
        "handler_duration_sec": time.time() - start,
        "init_duration_sec": init_duration,
    }
    return result


def handler(event, context):
    """AWS Lambda handler

    event:
    - query: str (base64)
    """
    try:
        result = handle_event(event)
    except Exception:
        result = {"exception": traceback.format_exc()}
    return result


if __name__ == "__main__":
    ballista_cmd = f"""
CREATE EXTERNAL TABLE trips01 STORED AS PARQUET
 LOCATION 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) FROM trips01
 GROUP BY payment_type;"""
    res = handler(
        {"query": base64.b64encode(ballista_cmd.encode("utf-8"))},
        {},
    )
    print(res)
