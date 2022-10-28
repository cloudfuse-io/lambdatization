## Notes:

- To simulate to the best the Lambda environement locally, we set flags such as
  `read_only` (except for `/tmp`) and a non root user in Docker Compose. We also
  set the relevant environment variables that are present in AWS Lambda:
  - token based credentials (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` and
    `AWS_SESSION_TOKEN`)
  - `AWS_REGION` or `AWS_DEFAULT_REGION` (both are set in Lambda)
- We use the Python AWS Lambda Runtime Interface Client, but it requires adding
  Python to the base image. A Rust based Interface Client would spare us a few
  dozen MBs for most engines. This is not critical for most engines as the image
  loading (1 to 2 seconds) is by far dominated by the engine init duration and
  choping off a few MBs will not decrease that duration significantly.
