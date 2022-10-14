# Ballista lambdatization tricks

## List of tricks

- We clone the arrow-ballista repo for a given version. 
- Since there are not official docker images for ballista, but, inside the repo
  there are Dockerfiles for each part (builder, scheduler, executor) we've merged
  this images into one Dockerfile and passed the entry points into the lambda 
  function.
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
- We execute the query using ballista-cli
- We need to `cd` to `/tmp` before executing the query command so that the
  workdir is writeable.