import base64
import logging
import os
import selectors
import socket
import subprocess
import sys
import tempfile
import threading
import time
import traceback
from contextlib import closing

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True
INTERPOLATED_CONFIG_FILE = "/tmp/interpolated-config.xml"


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
        if duration >= 5:
            raise Exception(f"{process_name} timed out after {c} connection attempts")
        time.sleep(0.05)


def interpolate_config(config_str: str, virtual_ip: str, cluster_size: int) -> str:
    """Map virtual IPs to shards using the LSB"""
    current_replica = int(virtual_ip.split(".")[-1])
    if current_replica > cluster_size:
        raise Exception(f"Unexpected ip {virtual_ip} for cluster size {cluster_size}")
    cluster_hosts = [
        "localhost" if h == current_replica else f"172.28.0.{h}"
        for h in range(1, cluster_size + 1)
    ]
    replicas = [
        f"<replica><host>{h}</host><port>9000</port></replica>" for h in cluster_hosts
    ]
    logging.info(f"replicas interpolation: {replicas}")
    config_str = config_str.replace("%%REPLICA_LIST%%", "\n".join(replicas))
    #
    # config_str = config_str.replace(
    #     "%%MACROS_CONTENT%%", f"<replica>{current_replica:02d}</replica>"
    # )
    return config_str


def init() -> StdLogger:
    srv_proc = subprocess.Popen(
        ["clickhouse-server", f"--config-file={INTERPOLATED_CONFIG_FILE}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    logging.info("server starting...")
    logger = StdLogger()
    logger.add("server|stdout", srv_proc.stdout)
    logger.add("server|stderr", srv_proc.stderr)
    logger.start()
    try:
        wait_for_socket("server", 9000)
    except Exception as e:
        logging.error(str(e))
        if srv_proc.returncode is None:
            logging.info(f"clickhouse-server still running, terminating...")
            srv_proc.terminate()
        logging.info(f"clickhouse-server returncode: {srv_proc.wait(3)}")

        with open("/tmp/var/log/clickhouse-server/clickhouse-server.log", "r") as f:
            logging.info("/tmp/var/log/clickhouse-server/clickhouse-server.log")
            logging.info(f.read())
        raise
    return logger


def handle_event(event):
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    if not is_cold_start:
        raise Exception(f"Only cold starts supported")

    # TODO: config interpolation should work regardless of the subnet structure
    cluster_size = int(os.environ["CHAPPY_CLUSTER_SIZE"])
    virtual_ip = os.environ["CHAPPY_VIRTUAL_IP"]
    with open("/etc/clickhouse-server/config.xml", "r") as file:
        config_str = file.read()
    config_str = interpolate_config(config_str, virtual_ip, cluster_size)
    with open(INTERPOLATED_CONFIG_FILE, "w") as file:
        config_str = file.write(config_str)

    init()
    init_duration = time.time() - start

    resp = ""
    logs = ""
    src_command = ""
    if "query" in event and event["query"] != "":
        src_command = base64.b64decode(event["query"]).decode("utf-8")
        try:
            cli_proc = subprocess.run(
                [
                    "clickhouse-client",
                    f"--config-file={INTERPOLATED_CONFIG_FILE}",
                    "-q",
                    src_command,
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=event["timeout_sec"],
            )
            resp = cli_proc.stdout.decode()
            logs = cli_proc.stderr.decode()
        except subprocess.TimeoutExpired as e:
            assert e.stdout is not None, "not None if PIPE specified"
            assert e.stderr is not None, "not None if PIPE specified"
            resp = e.stdout.decode()
            logs = e.stderr.decode()

    else:
        timeout = event["timeout_sec"]
        logging.info(f"no query, running node for {timeout} sec")
        time.sleep(timeout)

    result = {
        "resp": resp,
        "logs": logs,
        "parsed_queries": [src_command],
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
            "init_duration_sec": init_duration,
        },
    }
    return result


def handler(event, context):
    """AWS Lambda handler

    event:
    - timeout_sec: float
    - env: dict
    - query: Optionl[str] (base64)
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
