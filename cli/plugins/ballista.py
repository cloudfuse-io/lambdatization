"""Ballista on AWS Lambda"""

from invoke import task
import core


@task
def lambda_example(c, path_to_folder="nyc-taxi/2019/01"):
    """CREATE EXTERNAL TABLE and find out SUM(trip_distance) GROUP_BY payment_type"""
    ballista_cmd = f"""
CREATE EXTERNAL TABLE trips STORED AS PARQUET
LOCATION 's3://{core.bucket_name(c)}/{path_to_folder}/';
SELECT payment_type, SUM(trip_distance) FROM trips
GROUP BY payment_type;"""
    print(ballista_cmd)
    core.run_lambda(c, "ballista", ballista_cmd)
