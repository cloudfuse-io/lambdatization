"""Dremio on AWS Lambda"""

from invoke import task
import core


@task
def lambda_example(c):
    """SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
CREATE STAGE IF NOT EXISTS staging_1 
  url = 's3://testbucket/admin/data/' 
  credentials=(aws_key_id='minioadmin' aws_secret_key='minioadmin');

SELECT payment_type, SUM(trip_distance) 
FROM @staging_1
GROUP BY payment_type;
"""
    print(sql)
    core.run_lambda(c, "databend", sql)
