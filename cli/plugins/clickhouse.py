"""Clickhouse on AWS Lambda"""

import core
from common import AWS_REGION
from invoke import task


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type with direct FROM s3()"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3('https://{core.bucket_name(c)}.s3.{AWS_REGION()}.amazonaws.com/nyc-taxi/2019/{month}/*', 'Parquet')
GROUP BY payment_type"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "clickhouse", sql, json_output=json_output)
