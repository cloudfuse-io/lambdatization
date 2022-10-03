"""Dremio on AWS Lambda"""

from invoke import task
from common import TFDIR
import core


@task
def example(c):
    """SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3source."l12n-615900053518-eu-west-1-default"."nyc-taxi"."2019"."01" 
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "dremio", sql)


# @task
# def local(c):
#     python_cmd = "import foo; print foo.hello()"
#     c.run(f"cd {TFDIR}/dremio/build; docker compose down -v")
#     c.run(f"cd {TFDIR}/dremio/build; docker compose up")
