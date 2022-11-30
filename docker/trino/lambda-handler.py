import base64
import logging
import os
import selectors
import subprocess
import sys
import tempfile
import threading
import time
from typing import Tuple

logging.getLogger().setLevel(logging.INFO)


class StdLogger:
    """A class that efficiently multiplexes std streams into logs"""

    def _start_logging(self):
        while True:
            for key, _ in self.selector.select():
                # read1 instead or read to avoid blocking
                data = key.fileobj.read1()
                if key.fileobj not in self.files:
                    raise Exception("Unexpected file desc in selector")
                with self.lock:
                    name = self.files[key.fileobj]
                if not data:
                    print(f"{name} - EOS", flush=True, file=sys.stderr)
                    self.selector.unregister(key.fileobj)
                    with self.lock:
                        del self.files[key.fileobj]
                else:
                    lines = data.decode().splitlines()
                    for line in lines:
                        print(f"{name} - {line}", flush=True, file=sys.stderr)
                    with self.lock:
                        self.logs[name].extend(lines)

    def __init__(self):
        self.lock = threading.Lock()
        self.files = {}
        self.logs = {}
        self.selector = selectors.DefaultSelector()
        self.thread = threading.Thread(target=self._start_logging, daemon=True)

    def start(self):
        """Start consuming registered streams (if any) and logging them"""
        self.thread.start()

    def add(self, name: str, file):
        """Add a new stream with the given name"""
        with self.lock:
            self.files[file] = name
            self.logs[name] = []
        self.selector.register(file, selectors.EVENT_READ)

    def get(self, name: str) -> str:
        """Get the history of the stream for the given name"""
        with self.lock:
            return "\n".join(self.logs[name])


IS_COLD_START = True
STD_LOGGER: StdLogger = None


def init():
    global STD_LOGGER
    STD_LOGGER = StdLogger()
    STD_LOGGER.start()

    trino_proc = subprocess.Popen(
        ["launcher", "run"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    STD_LOGGER.add("trino-srv|stdout", trino_proc.stdout)

    schematool_proc = subprocess.Popen(
        ["schematool", "-initSchema", "-dbType", "derby"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd="/tmp",
    )
    STD_LOGGER.add("schematool|stdout", schematool_proc.stdout)
    STD_LOGGER.add("schematool|stderr", schematool_proc.stderr)
    if schematool_proc.wait() != 0:
        raise Exception("Hive schema seeding failed")
    hive_proc = subprocess.Popen(
        ["start-metastore"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd="/tmp"
    )
    STD_LOGGER.add("hive-srv|stdout", hive_proc.stdout)
    STD_LOGGER.add("hive-srv|stderr", hive_proc.stderr)

    for line_bytes in trino_proc.stderr:
        log_line = line_bytes.decode()
        print(f"trino-srv|stderr - {log_line}", flush=True, file=sys.stderr, end="")
        if "======== SERVER STARTED ========" in log_line:
            return
    raise Exception("Trino server didn't start successfully")


def query(sql: str) -> Tuple[str, str]:
    """Run a single SQL query using Trino cli"""
    with tempfile.NamedTemporaryFile(prefix="query", delete=False) as tmp:
        query_file = tmp.name
        tmp.write(sql.encode())

    cli_proc = subprocess.Popen(
        ["trino", f"--file={query_file}", "--progress"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    STD_LOGGER.add("trino-cli|stdout", cli_proc.stdout)
    STD_LOGGER.add("trino-cli|stderr", cli_proc.stderr)
    if cli_proc.wait() != 0:
        raise Exception(f"Query failed: {STD_LOGGER.get('trino-cli|stderr')}")
    return (STD_LOGGER.get("trino-cli|stdout"), STD_LOGGER.get("trino-cli|stderr"))


def handler(event, context):
    """AWS Lambda handler"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")

    (resp_stdout, resp_stderr) = query(src_command)

    result = {
        "resp": resp_stdout,
        "logs": resp_stderr,
        "parsed_queries": [src_command],
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
        },
    }
    return result


if __name__ == "__main__":
    query_str = f"""
CREATE TABLE hive.default.taxi201901 (trip_distance REAL, payment_type VARCHAR)
WITH (
  external_location = 's3a://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/',
  format = 'PARQUET'
);

SELECT payment_type, SUM(trip_distance)
FROM hive.default.taxi201901
GROUP BY payment_type;
"""
    res = handler(
        {"query": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
