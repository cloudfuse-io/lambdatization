import base64
import os
import subprocess
import logging
import sys
import shutil
import time


logging.getLogger().setLevel(logging.DEBUG)
__stdout__ = sys.stdout
process_s = 0
process_e = 0

def init():
    # start scheduler
    global process_s
    process_s = subprocess.Popen(["/opt/ballista/ballista-scheduler"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    logging.debug("scheduler starts")
    # wait till the scheduler is up and running
    return_code=1
    while return_code:
        res = subprocess.run(["nc", "-z", "localhost", "50050"], capture_output=True)
        return_code = res.returncode
        logging.debug(res)

    global process_e    
    process_e = subprocess.Popen(["/opt/ballista/ballista-executor", "-c" ,"4"],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    logging.debug("executor.starts")
    return_code=1
    while return_code:
        res = subprocess.run(["nc", "-z", "localhost", "50051"], capture_output=True)
        return_code = res.returncode
        logging.debug(res)


def kill():
    global process_e
    global process_s
    process_e.kill()
    process_s.kill()


IS_COLD_START = True


def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False

    start = time.time()

    if is_cold_start:
        init()
    init_duration = time.time() - start

    # input parameters
    logging.debug("event: %s", event)
    src_command = base64.b64decode(event["cmd"]).decode("utf-8")
    comm_file_path = '/tmp/commands.sql'
    with open(comm_file_path,'w') as file:
        file.write(src_command)
    command = ['/opt/ballista/ballista-cli', '--host', 'localhost', '--port', '50050', '-f', f'{comm_file_path}', '--format', 'csv']
    logging.info("command: %s", src_command)
    if "env" in event:
        logging.info("env: %s", event["env"])
        for (k, v) in event["env"].items():
            os.environ[k] = v
    
    process_cli = subprocess.run(command, capture_output=True)

    if process_cli.returncode != 0:
        return f"{command} exited with code {process_cli.returncode}:\n{process_cli.stdout}{process_cli.stderr}"
    logging.info("returncode: %s", process_cli.returncode)
    result = {
        "stdout": process_cli.stdout,
        "stderr": process_cli.stderr,
        "returncode": process_cli.returncode,
        "env": event.get("env", {}),
        "context": {
            "cold_start": is_cold_start,
            "handler_duration_sec": time.time() - start,
            "init_duration_sec": init_duration,
        },
    }
    return result


if __name__ == "__main__":
    ballista_cmd = f"""
CREATE EXTERNAL TABLE trips STORED AS PARQUET
LOCATION 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) FROM trips
GROUP BY payment_type;"""
    res = handler(
        {"cmd": base64.b64encode(ballista_cmd.encode("utf-8"))},
        {},
    )
    print(res)
    kill()

