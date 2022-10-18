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
- We execute the query using ballista-cli
