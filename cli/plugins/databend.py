"""Dremio on AWS Lambda"""

from invoke import task
import core


@task
def lambda_example(c, json_output=False):
    """SUM(trip_distance) GROUP_BY payment_type"""
    # NOTE: __RUNTIME_PROVIDED__ is interpolated by the handler with actual credentials
    sql = f"""
CREATE TRANSIENT TABLE IF NOT EXISTS taxi201901
(
    payment_type VARCHAR,
    trip_distance FLOAT
);

COPY INTO taxi201901
  FROM 's3://{core.bucket_name(c)}/nyc-taxi/2019/01/'
  credentials=(__RUNTIME_PROVIDED__)
  pattern ='.*[.]parquet'
  file_format = (type = 'PARQUET');

SELECT payment_type, SUM(trip_distance) 
FROM taxi201901
GROUP BY payment_type;
"""
    if not json_output:
        print(sql)
    core.run_lambda(c, "databend", sql, json_output=json_output)
