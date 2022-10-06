"""Dremio on AWS Lambda"""

from invoke import task
import core


@task
def example(c, month="01"):
    """SUM(trip_distance) GROUP_BY payment_type"""
    sql = f"""
SELECT payment_type, SUM(trip_distance) 
FROM s3source."{core.bucket_name(c)}"."nyc-taxi"."2019"."{month}"
GROUP BY payment_type
"""
    print(sql)
    core.run_lambda(c, "dremio", sql)


# @task
# def local(c):
#     python_cmd = "import foo; print foo.hello()"
#     c.run(f"cd {TFDIR}/dremio/build; docker compose down -v")
#     c.run(f"cd {TFDIR}/dremio/build; docker compose up")
