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

def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    shutil.rmtree("/tmp", ignore_errors=True)
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    # input parameters
    logging.debug("event: %s", event)
    command = f'/home/builder/workspace/target/release/ballista-cli -f commands.sql --format tsv'
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
    çomm_file_path = '/tmp/commands.sql'
    with open(çomm_file_path,'w') as file:
        file.write(f"""CREATE EXTERNAL TABLE trips STORED AS PARQUET LOCATION 's3://{os.getenv("DATA_BUCKET_NAME")}';
SELECT payment_type, SUM(trip_distance) FROM trips GROUP BY payment_type;""")
    
    res = handler(
        {"cmd": çomm_file_path},
        {},
    )
    print(res)
