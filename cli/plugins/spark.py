"""Spark on AWS Lambda"""

from invoke import task
import core


@task(autoprint=True)
def lambda_example_hive(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type with preliminary CREATE EXTERNAL TABLE"""
    sql = f"""
CREATE EXTERNAL TABLE nyc{month} (trip_distance FLOAT, payment_type STRING) 
STORED AS PARQUET LOCATION 's3a://{core.bucket_name(c)}/nyc-taxi/2019/{month}/';
SELECT payment_type, SUM(trip_distance) 
FROM nyc{month} 
GROUP BY payment_type
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "spark", sql, json_output=json_output)


@task
def lambda_example_direct(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type with direct FROM parquet.s3a://"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM parquet.\`s3a://{core.bucket_name(c)}/nyc-taxi/2019/{month}/\` 
GROUP BY payment_type
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "spark", sql, json_output=json_output)
