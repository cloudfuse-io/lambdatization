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


logging.getLogger().setLevel(logging.DEBUG)
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
    # start scheduler and executor
    proces_s = subprocess.Popen(["/opt/ballista/ballista-scheduler"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    # we need to spin up a thread to avoid deadlock when reading through output pipes
    stderr_thread_s = ReturningThread(
        target=buff_and_print, args=(process_s.stderr, "stderr")
    )
    stderr_thread_s.start()
    stdout_s = buff_and_print(process_s.stdout, "stdout").strip()
    stderr_s = stderr_thread_s.join().strip()
    process_e = subprocess.Popen(["/opt/ballista/ballista-executor -c -4"],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    stderr_thread_e = ReturningThread(
        target=buff_and_print, args=(process_e.stderr, "stderr")
    )
    stderr_thread_e.start()
    stdout_e = buff_and_print(process_e.stdout, "stdout").strip()
    stderr_e = stderr_thread_e.join().strip()
    # input parameters
    logging.debug("event: %s", event)
    çomm_file_path = '/tmp/commands.sql'
    with open(çomm_file_path,'w') as file:
        file.write(f"""CREATE EXTERNAL TABLE trips STORED AS PARQUET LOCATION 's3://{os.getenv("+_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) FROM trips GROUP BY payment_type;""")
    command = f'/opt/ballista/ballista-cli -f {çomm_file_path} --format tsv'
    logging.info("command: %s", command)
    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v
    process_cli = subprocess.run(command, capture_output=True)
    if process_cli.returncode != 0:
        return f"{command} exited with code {process_cli.returncode}:\n{process_cli.stdout}{process_cli.stderr}"
    logging.info("returncode: %s", returncode)
    result = {
        "stdout": stdout,
        "stderr": stderr,
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
    
    res = handler(
        {},
        {},
    )
    print(res)
