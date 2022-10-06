import os
import subprocess
import logging
import base64
import sys
import io
import json
import shutil
import time
from threading import Thread

logging.getLogger().setLevel(logging.INFO)
__stdout__ = sys.stdout

IS_COLD_START = True


class ReturningThread(Thread):
    """A wrapper around the Thread class to actually return the threaded function
    return value when calling join()"""

    def __init__(self, target=None, args=()):
        Thread.__init__(self, target=target, args=args)
        self._return = None

    def run(self):
        if self._target is not None:
            self._return = self._target(*self._args, **self._kwargs)

    def join(self, *args):
        Thread.join(self, *args)
        return self._return


class CommandException(Exception):
    ...


def hide_command_exception(func):
    """Block printing on CommandException to avoid logging stderr twice"""

    def func_wrapper(*args, **kwargs):
        # reset stdout to its original value that we saved on init
        sys.stdout = __stdout__
        try:
            return func(*args, **kwargs)
        except CommandException as e:
            # the error will be printed to a disposable buffer
            sys.stdout = io.StringIO()
            raise e

    return func_wrapper


def buff_and_print(stream, stream_name):
    """Buffer and log every line of the given stream"""
    buff = []
    for l in iter(lambda: stream.readline(), b""):
        line = l.decode("utf-8")
        logging.info("%s: %s", stream_name, line.rstrip())
        buff.append(line)
    return "".join(buff)


@hide_command_exception
def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    shutil.rmtree("/tmp", ignore_errors=True)
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    # input parameters
    logging.debug("event: %s", event)
    sql_command = base64.b64decode(event["cmd"]).decode("utf-8")
    sql_command_escaped = sql_command.replace('"', '\\"')
    command = f'cd /tmp; /opt/spark/bin/spark-sql -e "{sql_command_escaped}" '
    logging.info("command: %s", command)
    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v

    # execute the command as bash and return the std outputs
    parsed_cmd = ["/bin/bash", "-c", command]
    process = subprocess.Popen(
        parsed_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    # we need to spin up a thread to avoid deadlock when reading through output pipes
    stderr_thread = ReturningThread(
        target=buff_and_print, args=(process.stderr, "stderr")
    )
    stderr_thread.start()
    stdout = buff_and_print(process.stdout, "stdout").strip()
    stderr = stderr_thread.join().strip()
    returncode = process.wait()
    logging.info("returncode: %s", returncode)
    result = {
        "stdout": stdout,
        "stderr": stderr,
        "parsed_cmd": parsed_cmd,
        "returncode": returncode,
        "env": event.get("env", {}),
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
        },
    }
    if returncode != 0:
        raise CommandException(json.dumps(result))
    return result


if __name__ == "__main__":
    query_str = f"""
SELECT payment_type, SUM(trip_distance) 
FROM parquet.\`s3a://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/\` 
GROUP BY payment_type
"""
    res = handler(
        {"cmd": base64.b64encode(query_str.encode("utf-8"))},
        {},
    )
    print(res)
