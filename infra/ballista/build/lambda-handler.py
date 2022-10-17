import os
import subprocess
import logging
import sys
import shutil
import time


logging.getLogger().setLevel(logging.DEBUG)
__stdout__ = sys.stdout

IS_COLD_START = True

def handler(event, context):
    """An AWS Lambda handler that runs the provided command with bash and returns the standard output"""
    shutil.rmtree("/tmp", ignore_errors=True)
    start = time.time()
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    # start scheduler
    process_s = subprocess.Popen(["/opt/ballista/ballista-scheduler"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    logging.debug("scheduler starts")
    # wait till the scheduler is up and running
    return_code=1
    while return_code:
        res = subprocess.run(["nc", "-z", "localhost", "50050"], capture_output=True)
        return_code = res.returncode
        logging.debug(res)
        
    process_e = subprocess.Popen(["/opt/ballista/ballista-executor", "-c" ,"4"],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    logging.debug("executor.starts")
    return_code=1
    while return_code:
        res = subprocess.run(["nc", "-z", "localhost", "50051"], capture_output=True)
        return_code = res.returncode
        logging.debug(res)
    # input parameters
    logging.debug("event: %s", event)
    çomm_file_path = '/tmp/commands.sql'
    with open(çomm_file_path,'w') as file:
        file.write(f"""CREATE EXTERNAL TABLE trips STORED AS PARQUET LOCATION 's3://{os.getenv("DATA_BUCKET_NAME")}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) FROM trips GROUP BY payment_type;""")
    command = ['/opt/ballista/ballista-cli', '--host', 'localhost', '--port', '50050', '-f', f'{çomm_file_path}', '--format', 'csv']
    logging.info("command: %s", ' '.join(command))
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
        },
    }
    process_e.kill()
    process_s.kill()
    return result


if __name__ == "__main__":
    
    res = handler(
        {},
        {},
    )
    print(res)

