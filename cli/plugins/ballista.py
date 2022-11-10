"""Ballista on AWS Lambda"""

import core
from invoke import task


@task(autoprint=True)
def lambda_example(c, month="01"):
    """CREATE EXTERNAL TABLE and find out SUM(trip_distance) GROUP_BY payment_type"""
    ballista_cmd = f"""
CREATE EXTERNAL TABLE nyctaxi2019{month} STORED AS PARQUET
LOCATION 's3://{core.bucket_name(c)}/nyc-taxi/2019/{month}/';
SELECT payment_type, SUM(trip_distance) FROM nyctaxi2019{month}
GROUP BY payment_type;"""
    print(ballista_cmd)
    return core.run_lambda(c, "ballista", ballista_cmd)
