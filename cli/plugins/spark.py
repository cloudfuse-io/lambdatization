"""Spark on AWS Lambda"""

from invoke import task
import core


@task
def lambda_example_hive(c):
    """SUM(trip_distance) GROUP_BY payment_type with preliminary CREATE EXTERNAL TABLE"""
    sql = f"""
CREATE EXTERNAL TABLE nyc (trip_distance FLOAT, payment_type STRING) 
STORED AS PARQUET LOCATION 's3a://{core.bucket_name(c)}/nyc-taxi/2019/01/';
SELECT payment_type, SUM(trip_distance) 
FROM nyc 
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "spark", sql)


@task
def lambda_example_direct(c):
    """SUM(trip_distance) GROUP_BY payment_type with direct FROM parquet.s3a://"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM parquet.\`s3a://{core.bucket_name(c)}/nyc-taxi/2019/01/\` 
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "spark", sql)
