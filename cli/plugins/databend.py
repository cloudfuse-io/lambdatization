"""Databend on AWS Lambda"""

import core
from common import AWS_REGION
from invoke import task


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type"""
    # NOTE: __RUNTIME_PROVIDED__ is interpolated by the handler with actual credentials
    sql = f"""
CREATE STAGE IF NOT EXISTS taxi2019{month}
URL = 's3://{core.bucket_name(c)}/nyc-taxi/2019/{month}/'
CONNECTION = (
__RUNTIME_PROVIDED__
REGION = '{AWS_REGION()}'
)
FILE_FORMAT = (type = 'PARQUET');

SELECT payment_type, SUM(trip_distance) 
FROM @taxi2019{month}
GROUP BY payment_type;
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "databend", sql, json_output=json_output)
