"""Dremio on AWS Lambda"""

from invoke import task
from common import TFDIR
import core


@task
def lambda_example(c, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3source."{core.bucket_name(c)}"."nyc-taxi"."2019"."{month}"
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "dremio", sql)
