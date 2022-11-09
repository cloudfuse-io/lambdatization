"""Trino on AWS Lambda"""

import core
from invoke import task


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type with preliminary CREATE EXTERNAL TABLE"""
    sql = f"""
CREATE TABLE hive.default.taxi2019{month} (trip_distance REAL, payment_type VARCHAR)
WITH (
  external_location = 's3a://{core.bucket_name(c)}/nyc-taxi/2019/{month}/',
  format = 'PARQUET'
);

SELECT payment_type, SUM(trip_distance)
FROM hive.default.taxi2019{month}
GROUP BY payment_type;
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "trino", sql, json_output=json_output)
