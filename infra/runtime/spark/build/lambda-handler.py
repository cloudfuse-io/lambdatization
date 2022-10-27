from doctest import UnexpectedException
import os
import logging
import base64
import time
import sys
import textwrap
import io
import selectors
import subprocess
from typing import Tuple

logging.getLogger().setLevel(logging.INFO)


class CustomExpect:
    """A custom implementation of pexpect that treats stdout and stderr separately"""

    def __init__(self, command, cwd):
        p = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=cwd,
        )
        sel = selectors.DefaultSelector()
        sel.register(p.stdout, selectors.EVENT_READ)
        sel.register(p.stderr, selectors.EVENT_READ)
        self.selector = sel
        self.process = p

    def _excp(msg, stdout, stderr):
        """Create a formatted Exception for this class"""
        mess = f"""{msg}:
        
        STDOUT:
        {stdout}
        
        STDERR:
        {stderr}"""
        return Exception(textwrap.dedent(mess))

    def expect(self, expected: str, timeout: int) -> Tuple[str, str]:
        """Tests the string "expected" against stdout and returns the tuple (stdout,stderr)

        - The "expected" string is just tested whether it is "in" the last written
        bytes to the process stdout. This might not pick up the string if the
        process stdout blocked right in the middle of the pattern
        - The "timeout" is specified in seconds
        - The (stdout,stderr) returned tuple contains every output since the
        start of the process or the last call to this method"""
        start = time.time()
        before_stdout = io.BytesIO()
        before_stderr = io.BytesIO()
        while True:
            stdout_data = ""
            for key, _ in self.selector.select():
                if time.time() - start > timeout:
                    raise CustomExpect._excp(
                        f"Timeout: exceeded {timeout} seconds",
                        before_stdout.getvalue().decode(),
                        before_stderr.getvalue().decode(),
                    )
                # read1 instead or read to avoid blocking
                data = key.fileobj.read1()
                if not data:
                    raise CustomExpect._excp(
                        "EOF: Reached process output end",
                        before_stdout.getvalue().decode(),
                        before_stderr.getvalue().decode(),
                    )
                if key.fileobj is self.process.stdout:
                    data_str = data.decode()
                    print(data_str, end="")
                    before_stdout.write(data)
                    stdout_data = data_str
                elif key.fileobj is self.process.stderr:
                    before_stderr.write(data)
                    print(data.decode(), end="", file=sys.stderr)
                else:
                    raise UnexpectedException("file desc should be stdout or stderr")
            if expected in stdout_data:
                stdout = before_stdout.getvalue().decode()
                stderr = before_stderr.getvalue().decode()
                return (stdout, stderr)


IS_COLD_START = True
CLI_EXPECT: CustomExpect = None


def init():
    global CLI_EXPECT
    CLI_EXPECT = CustomExpect(
        ["/opt/spark/bin/spark-sql"],
        cwd="/tmp",
    )
    CLI_EXPECT.expect("spark-sql>", timeout=120)
    logging.info("Spark SQL CLI started")


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    if is_cold_start:
        init()

    # input parameters
    logging.debug("event: %s", event)
    sql_command = base64.b64decode(event["cmd"]).decode("utf-8")

    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v

    resp_stdout = ""
    resp_stderr = ""
    for command in sql_command.split(";"):
        # CLI will hang if the request doesn't end with a semicolon and a newline
        # newline at the beginning is for helping the stdout cleanup
        command = f"\n{command.strip()};\n"
        if command == "\n;\n":
            continue
        logging.info("command: %s", command)

        # submit query to CLI
        CLI_EXPECT.process.stdin.write(command.encode())
        CLI_EXPECT.process.stdin.flush()
        stdout, stderr = CLI_EXPECT.expect("spark-sql>", timeout=240)

        # stdout also contains the input, so we have to clean it up
        resp_stdout = "\n".join(
            filter(
                lambda l: not l.startswith("         >")
                and not l in ["spark-sql> ", ""],
                stdout.split("\n"),
            )
        )

        # Only way to check if a query is successful
        if "Time taken:" not in stderr:
            raise Exception(f"Query failed: {stderr}")

        resp_stderr += stderr

    result = {
        "stdout": resp_stdout,
        "stderr": resp_stderr,
        "parsed_cmd": sql_command,
        "returncode": 0,
        "env": event.get("env", {}),
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
        },
    }
    return result


if __name__ == "__main__":
    query_str = f"""
CREATE EXTERNAL TABLE taxi201901 (trip_distance FLOAT, payment_type STRING) 
STORED AS PARQUET LOCATION 's3a://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) 
FROM taxi201901 
GROUP BY payment_type
"""
    res = handler(
        {"cmd": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
