"""Dremio on AWS Lambda"""

import core
from invoke import task


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3source."{core.bucket_name(c)}"."nyc-taxi"."2019"."{month}"
GROUP BY payment_type
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "dremio", sql, json_output=json_output)
