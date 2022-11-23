import base64
import os
import subprocess
import logging
import time
import sys
import socket
import threading
import selectors
from contextlib import closing


logging.getLogger().setLevel(logging.INFO)

process_cli = None
IS_COLD_START = True


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
        if duration >= 20:
            raise Exception(f"{process_name} timed out after {c} connection attempts")
        time.sleep(0.05)


def init():
    srv_proc = subprocess.Popen(
        ["clickhouse-server", "--config-file=/etc/clickhouse-server/config.xml"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    logging.info("server starting...")
    logger = StdLogger()
    logger.add("server|stdout", srv_proc.stdout)
    logger.add("server|stderr", srv_proc.stderr)
    logger.start()
    with open("/proc/self/auxv", "rb") as f:
        logging.info("/proc/self/auxv")
        logging.info(f"{f.read()}")
    try:
        wait_for_socket("server", 9000)
    except:
        with open("/tmp/var/log/clickhouse-server/clickhouse-server.log", "r") as f:
            logging.info("/tmp/var/log/clickhouse-server/clickhouse-server.log")
            logging.info(f.read())
        raise


def query(sql: str) -> str:
    subprocess.run(["clickhouse-client", "-q", sql])


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if is_cold_start:
        init()
    src_command = base64.b64decode(event["query"]).decode("utf-8")
    init_duration = time.time() - start

    cli_proc = subprocess.run(
        ["clickhouse-client", "-q", src_command],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    result = {
        "resp": cli_proc.stdout.decode(),
        "logs": cli_proc.stderr.decode(),
        "parsed_queries": [src_command],
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
            "init_duration_sec": init_duration,
        },
    }
    return result


if __name__ == "__main__":
    ballista_cmd = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3('https://{os.getenv("DATA_BUCKET_NAME")}.s3.{os.getenv("AWS_REGION")}.amazonaws.com/nyc-taxi/2019/01/*', 'Parquet')
GROUP BY payment_type"""
    res = handler(
        {"query": base64.b64encode(ballista_cmd.encode("utf-8"))},
        {},
    )
    print(res)
