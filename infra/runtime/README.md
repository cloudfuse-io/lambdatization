## Notes:

- To simulate to the best the Lambda environement, we set flags such as
  `read_only` (except for `/tmp`) and a non root user in Docker Compose. We also
  pass AWS credentials as environement variables as Spark does not support the
  credentials file (set `L12N_S3_AWS_ACCESS_KEY_ID` and
  `L12N_S3_AWS_SECRET_ACCESS_KEY` in you `.env` file and run `./l12n-shell`
  again). To navigate the resulting image with this "best effort" simulation
  settings, use `docker compose run engine_name`
- We use the Python AWS Lambda Runtime Interface Client, but it requires adding
  Python to the base image. A Rust based Interface Client would spare us a few
  dozen MBs for most engines. This is not critical for most engines as the image
  loading (1 to 2 seconds) is by far dominated by the engine init duration and
  choping off a few MBs will not decrease that duration significantly.
