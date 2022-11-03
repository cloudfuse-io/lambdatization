# Trino lambdatization tricks

## List of tricks

- Trino loads many plugins by default, which implies opening many jar files in
  parallel. To make sure this process doesn't exceed the system's maximum number
  of file descriptors, it has a check on the ulimit for file descriptor that
  cannot be disabled through configuration. The minimum set is 4096 but we have
  a hard limit on AWS Lambda at 1024. We had to
  [rebuild](https://github.com/cloudfuse-io/lambdatization/actions/workflows/trino.yaml)
  Trino with a patch that:
    - loads less plugins
    - removes the check on fileno
- It seems you cannot query S3 without using the Hive metastore, so we had to
  install a local version of it running on Derby which adds to the init time.
- The container image is huge (>2GB):
  - we are pulling in a full Hadoop distribution, in which most files won't be
    used. We could reduce the image size by at least 500MB by cherry picking the
    dependencies from it
  - we could also use a remote Hive metastore (like Glue) instead of installing
    a local one
  - obviously, we could use a smaller base image
