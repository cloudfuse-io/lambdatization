"""Dask on AWS Lambda"""

from invoke import task
import core


@task(autoprint=True)
def lambda_example(c, json_output=False, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
CREATE TABLE nyctaxi2019{month} WITH (
    location = "s3://{core.bucket_name(c)}/nyc-taxi/2019/{month}/*",
    format = "parquet"
);

SELECT payment_type, SUM(trip_distance) 
FROM nyctaxi2019{month}
GROUP BY payment_type
"""
    if not json_output:
        print(sql)
    return core.run_lambda(c, "dask", sql, json_output=json_output)
