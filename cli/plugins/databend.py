"""Dremio on AWS Lambda"""

from invoke import task
import core


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type"""
    # NOTE: __RUNTIME_PROVIDED__ is interpolated by the handler with actual credentials
    sql = f"""
CREATE TRANSIENT TABLE IF NOT EXISTS taxi2019{month}
(
    payment_type VARCHAR,
    trip_distance FLOAT
);

COPY INTO taxi2019{month}
  FROM 's3://{core.bucket_name(c)}/nyc-taxi/2019/{month}/'
  credentials=(__RUNTIME_PROVIDED__)
  pattern ='.*[.]parquet'
  file_format = (type = 'PARQUET');

SELECT payment_type, SUM(trip_distance) 
FROM taxi2019{month}
GROUP BY payment_type;
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "databend", sql, json_output=json_output)
