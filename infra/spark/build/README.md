# Spark lambdatization tricks

## List of tricks

- We use the official Spark image as base. It is pretty lightweight (could be
  even lighter?) but doesn't contain the jars necessary to read from S3
- We grab the Hadoop and AWS SDK jars from Maven Central by following the class not found errors.
- We use the Python AWS Lambda Runtime Interface Client, but it requires adding
  Python to the base image. A Rust based Interface Client would spare us a few
  dozen MBs.
- To simulate to the best the Lambda environement, we set flags such as
  `read_only` (except for `/tmp`) and a non root user in Docker Compose. We also
  pass AWS credentials as environement variables as Spark does not support the
  credentials file (set `L12N_S3_AWS_ACCESS_KEY_ID` and
  `L12N_S3_AWS_SECRET_ACCESS_KEY` in you `.env` file and run `./l12n-shell`
  again). To navigate the resulting image with this "best effort" simultion
  settings, use `docker compose run spark`
- We need to `cd` to `/tmp` before executing the `spark-sql` command so that the
  workdir is writeable
- We override `spark-class` because it uses process substitution (`<(...)`)
  which is using `/dev/fd/63` as a tmp file and that is not allowed inside
  Lambda
- We set `spark.driver.bindAddress` to `localhost`, otherwise the port binding
  fails in Lambda

## Changing Spark Version

Because of all the tweeks listed above, using a different Spark version would
require reconfiguring many elements:

- set compatible Hadoop and AWS SDK versions
- update `spark-class` file if it changed
