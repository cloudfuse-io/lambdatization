# Spark lambdatization tricks

## List of tricks

- We use the official Spark image as base. It is pretty lightweight (could be
  even lighter?) but doesn't contain the jars necessary to read from S3
- We grab the Hadoop and AWS SDK jars from Maven Central by following the class
  not found errors.
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
