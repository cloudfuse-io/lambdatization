"""Spark on AWS Lambda"""

from invoke import task
import core


@task
def example_hive(c):
    """SUM(trip_distance) GROUP_BY payment_type with preliminary CREATE EXTERNAL TABLE"""
    sql = """
CREATE EXTERNAL TABLE nyc (trip_distance FLOAT, payment_type STRING) 
STORED AS PARQUET LOCATION 's3a://ursa-labs-taxi-data/2019/06/';
SELECT payment_type, SUM(trip_distance) 
FROM nyc 
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "spark", sql)


@task
def example_direct(c):
    """SUM(trip_distance) GROUP_BY payment_type with direct FROM parquet.s3a://"""
    sql = """
SELECT payment_type, SUM(trip_distance) 
FROM parquet.`s3a://ursa-labs-taxi-data/2019/06/` 
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "spark", sql)
